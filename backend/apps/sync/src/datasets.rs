//! One-shot dataset refresh phase. Scheduling is owned by the operator.
//!
//! [`refresh_all`] refreshes every attachment once, up to a configured number
//! at a time. Each attachment is refreshed under its own lock with one
//! Caboodle connection; any error fails that run and the others continue.

use std::num::NonZeroUsize;
use std::time::Duration;

use futures::{StreamExt, stream};
use infra_caboodle::{CaboodleClient, CaboodleConfig, CaboodleError, datasets::execute_dataset};
use infra_pg::{
    PgDatasetRepository,
    datasets::{RefreshRun, RefreshStoreError, RefreshTarget},
};
use pocr_core::{
    dataset::{BatchError, DatasetDefinition, DatasetRegistry, batches},
    environment::Environment,
};
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

/// Settings for one refresh pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshSettings {
    /// Environment whose assignments are refreshed.
    pub environment: Environment,
    /// Most attachments refreshed at once.
    pub concurrency: NonZeroUsize,
    /// Time limit for each stored procedure call.
    pub procedure_timeout: Duration,
}

/// Counts of attachments by outcome for one refresh pass.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefreshSummary {
    /// Snapshots published.
    pub published: usize,
    /// Attachments left alone: another refresh holds the lock, or the
    /// dataset's definition is no longer deployed.
    pub skipped: usize,
    /// Attachments whose refresh failed.
    pub failed: usize,
}

#[derive(Debug)]
enum RefreshOutcome {
    Published { run_id: Uuid, row_count: u64 },
    AlreadyRunning,
    DefinitionUnavailable,
}

#[derive(Debug, thiserror::Error)]
enum RefreshError {
    #[error(transparent)]
    Store(#[from] RefreshStoreError),
    #[error(transparent)]
    Source(#[from] CaboodleError),
    #[error(transparent)]
    Batch(#[from] BatchError),
}

/// Refresh every attachment once in the settings' environment, running at
/// most `concurrency` refreshes at a time.
///
/// A missing definition is an expected state and counts as skipped. Other
/// failures, including attachments with corrupt stored names, are logged,
/// counted as failed, and do not stop later attachments.
///
/// # Errors
///
/// Returns [`RefreshStoreError`] only when the attachments cannot be listed.
pub async fn refresh_all(
    repository: &PgDatasetRepository,
    registry: &DatasetRegistry,
    caboodle: &CaboodleConfig,
    settings: RefreshSettings,
) -> Result<RefreshSummary, RefreshStoreError> {
    let mut summary = RefreshSummary::default();
    let mut targets = Vec::new();
    for target in repository.refresh_targets().await? {
        match target {
            Ok(target) => targets.push(target),
            Err(error) => {
                warn!(error_message = %error, "dataset attachment is corrupt; refresh skipped");
                summary.failed += 1;
            }
        }
    }
    let mut outcomes = stream::iter(&targets)
        .map(|target| refresh_one(repository, registry, caboodle, settings, target))
        .buffer_unordered(settings.concurrency.get());
    while let Some(outcome) = outcomes.next().await {
        match outcome {
            Ok(RefreshOutcome::Published { .. }) => summary.published += 1,
            Ok(RefreshOutcome::AlreadyRunning | RefreshOutcome::DefinitionUnavailable) => {
                summary.skipped += 1;
            }
            Err(_) => summary.failed += 1,
        }
    }
    info!(
        published = summary.published,
        skipped = summary.skipped,
        failed = summary.failed,
        concurrency = settings.concurrency.get(),
        environment = %settings.environment,
        "dataset refresh finished"
    );
    Ok(summary)
}

#[instrument(
    skip_all,
    fields(
        attachment_id = %target.attachment_id,
        dataset_name = %target.dataset_name,
        environment = %settings.environment,
        run_id = tracing::field::Empty,
    )
)]
async fn refresh_one(
    repository: &PgDatasetRepository,
    registry: &DatasetRegistry,
    caboodle: &CaboodleConfig,
    settings: RefreshSettings,
    target: &RefreshTarget,
) -> Result<RefreshOutcome, RefreshError> {
    let outcome = refresh_target(repository, registry, caboodle, settings, target).await;
    match &outcome {
        Ok(RefreshOutcome::Published { run_id, row_count }) => {
            info!(%run_id, row_count, "dataset refresh published");
        }
        Ok(RefreshOutcome::AlreadyRunning | RefreshOutcome::DefinitionUnavailable) => {}
        Err(error) => error!(error_message = %error, "dataset refresh failed"),
    }
    outcome
}

async fn refresh_target(
    repository: &PgDatasetRepository,
    registry: &DatasetRegistry,
    caboodle: &CaboodleConfig,
    settings: RefreshSettings,
    target: &RefreshTarget,
) -> Result<RefreshOutcome, RefreshError> {
    let Some(definition) = registry.get(&target.dataset_name) else {
        warn!("dataset definition unavailable; refresh skipped");
        return Ok(RefreshOutcome::DefinitionUnavailable);
    };
    let Some(lock) = repository
        .try_lock(target.attachment_id, settings.environment)
        .await?
    else {
        info!("dataset refresh already running");
        return Ok(RefreshOutcome::AlreadyRunning);
    };
    let mut run = lock.begin(definition).await?;
    let run_id = run.id();
    tracing::Span::current().record("run_id", tracing::field::display(run_id));
    match populate(&mut run, definition, caboodle, settings.procedure_timeout).await {
        Ok(()) => {
            let row_count = run.publish().await?;
            Ok(RefreshOutcome::Published { run_id, row_count })
        }
        Err(error) => {
            if let Err(fail_error) = run.fail().await {
                error!(error_message = %fail_error, "dataset run could not be marked failed");
            }
            Err(error)
        }
    }
}

/// Stage every cohort page. Caboodle is contacted on the first non-empty page,
/// so an empty cohort publishes without a source connection.
async fn populate(
    run: &mut RefreshRun,
    dataset: &DatasetDefinition,
    caboodle: &CaboodleConfig,
    procedure_timeout: Duration,
) -> Result<(), RefreshError> {
    let mut connection: Option<CaboodleClient> = None;
    loop {
        let page = run.next_page().await?;
        if page.is_empty() {
            return Ok(());
        }
        let client = match connection.as_mut() {
            Some(client) => client,
            None => connection.insert(CaboodleClient::connect(caboodle).await?),
        };
        for batch in batches(&page)? {
            let records = execute_dataset(client, dataset, batch, procedure_timeout).await?;
            run.stage(&records).await?;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pocr_core::dataset::{DatasetName, DatasetRepository, DatasetStatus};
    use secrecy::SecretString;
    use sqlx::{
        Connection, PgConnection, PgPool,
        postgres::{PgConnectOptions, PgPoolOptions},
    };

    use super::*;

    const PRODUCTION: Environment = Environment::Production;

    fn settings(concurrency: usize) -> RefreshSettings {
        RefreshSettings {
            environment: PRODUCTION,
            concurrency: NonZeroUsize::new(concurrency).unwrap(),
            procedure_timeout: Duration::from_secs(1),
        }
    }

    /// A fresh database, so attachments from other tests never appear in `refresh_targets`.
    async fn isolated_repository() -> (PgDatasetRepository, PgPool) {
        let url = std::env::var("DATASET_TEST_DATABASE_URL")
            .expect("explicit disposable test database required");
        let options = PgConnectOptions::from_str(&url).expect("valid database URL");
        let name = format!("sync_test_{}", Uuid::now_v7().simple());
        let mut admin = PgConnection::connect_with(&options).await.unwrap();
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&mut admin)
            .await
            .unwrap();
        let pool = PgPoolOptions::new()
            .connect_with(options.database(&name))
            .await
            .unwrap();
        infra_pg::run_migrations(&pool).await.unwrap();
        (PgDatasetRepository::new(pool.clone()), pool)
    }

    fn fixture_registry() -> DatasetRegistry {
        infra_datasets::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/infra-datasets/tests/fixtures/catalog"
        )))
        .unwrap()
        .datasets()
        .clone()
    }

    fn fixture_definition() -> DatasetDefinition {
        fixture_registry()
            .get(&DatasetName::parse("sample-outcomes").unwrap())
            .unwrap()
            .clone()
    }

    fn unreachable_caboodle() -> CaboodleConfig {
        CaboodleConfig {
            host: "127.0.0.1".into(),
            port: 1,
            database: "unused".into(),
            username: "unused".into(),
            password: SecretString::from("unused"),
        }
    }

    async fn attach_new_study(repository: &PgDatasetRepository, pool: &PgPool) -> Uuid {
        let study = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO studies(id,name,randomization_config) VALUES ($1,'Sync test','{}')",
        )
        .bind(study)
        .execute(pool)
        .await
        .unwrap();
        repository
            .attach(study, &fixture_definition())
            .await
            .unwrap();
        study
    }

    async fn seed_randomization(pool: &PgPool, study: Uuid) {
        let randomization = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO randomizations(id,study_id,unit_hash,arm,environment)
            VALUES ($1,$2,$3,$4,'production')",
        )
        .bind(randomization)
        .bind(study)
        .bind(randomization.to_string())
        .bind(serde_json::json!({"id": Uuid::from_u128(100), "name": "Treatment", "ratio": 1}))
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
    async fn refresh_all_skips_attachment_whose_definition_is_missing() {
        let (repository, pool) = isolated_repository().await;
        attach_new_study(&repository, &pool).await;

        let summary = refresh_all(
            &repository,
            &DatasetRegistry::default(),
            &unreachable_caboodle(),
            settings(1),
        )
        .await
        .unwrap();

        assert_eq!(
            summary,
            RefreshSummary {
                published: 0,
                skipped: 1,
                failed: 0
            }
        );
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
    async fn refresh_all_continues_after_a_failed_attachment() {
        let (repository, pool) = isolated_repository().await;
        let first = attach_new_study(&repository, &pool).await;
        let second = attach_new_study(&repository, &pool).await;
        seed_randomization(&pool, first).await;
        seed_randomization(&pool, second).await;

        let summary = refresh_all(
            &repository,
            &fixture_registry(),
            &unreachable_caboodle(),
            settings(1),
        )
        .await
        .unwrap();

        assert_eq!(
            summary,
            RefreshSummary {
                published: 0,
                skipped: 0,
                failed: 2
            }
        );
        for study in [first, second] {
            let attachments = repository.list(study, PRODUCTION).await.unwrap();
            assert_eq!(attachments[0].status, DatasetStatus::Failed);
        }
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
    async fn refresh_all_refreshes_attachments_concurrently() {
        let (repository, pool) = isolated_repository().await;
        let studies = [
            attach_new_study(&repository, &pool).await,
            attach_new_study(&repository, &pool).await,
            attach_new_study(&repository, &pool).await,
        ];

        let summary = refresh_all(
            &repository,
            &fixture_registry(),
            &unreachable_caboodle(),
            settings(2),
        )
        .await
        .unwrap();

        assert_eq!(
            summary,
            RefreshSummary {
                published: 3,
                skipped: 0,
                failed: 0
            }
        );
        for study in studies {
            let attachments = repository.list(study, PRODUCTION).await.unwrap();
            assert!(attachments[0].snapshot.is_some());
        }
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
    async fn refresh_all_reports_nothing_to_do_without_attachments() {
        let (repository, _pool) = isolated_repository().await;

        let summary = refresh_all(
            &repository,
            &fixture_registry(),
            &unreachable_caboodle(),
            settings(1),
        )
        .await
        .unwrap();

        assert_eq!(summary, RefreshSummary::default());
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
    async fn refresh_all_publishes_an_empty_cohort_without_contacting_caboodle() {
        let (repository, pool) = isolated_repository().await;
        let study = attach_new_study(&repository, &pool).await;

        let summary = refresh_all(
            &repository,
            &fixture_registry(),
            &unreachable_caboodle(),
            settings(1),
        )
        .await
        .unwrap();

        assert_eq!(
            summary,
            RefreshSummary {
                published: 1,
                skipped: 0,
                failed: 0
            }
        );
        let attachments = repository.list(study, PRODUCTION).await.unwrap();
        let snapshot = attachments[0]
            .snapshot
            .as_ref()
            .expect("published snapshot");
        assert_eq!(snapshot.row_count, 0);
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
    async fn refresh_all_counts_a_corrupt_target_as_failed_and_refreshes_the_rest() {
        let (repository, pool) = isolated_repository().await;
        let study = attach_new_study(&repository, &pool).await;
        sqlx::query("INSERT INTO study_datasets(id,study_id,name) VALUES ($1,$2,'Not A Name!')")
            .bind(Uuid::now_v7())
            .bind(study)
            .execute(&pool)
            .await
            .unwrap();

        let summary = refresh_all(
            &repository,
            &fixture_registry(),
            &unreachable_caboodle(),
            settings(1),
        )
        .await
        .unwrap();

        assert_eq!(
            summary,
            RefreshSummary {
                published: 1,
                skipped: 0,
                failed: 1
            }
        );
    }
}
