//! Dataset persistence and refresh sessions against a disposable PostgreSQL
//! database named by `DATASET_TEST_DATABASE_URL`.

use std::time::Duration;

use infra_pg::{
    PgDatasetRepository,
    datasets::{DatabaseFailure, RefreshLock, RefreshRun, RefreshStoreError},
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use pocr_core::{
    dataset::{
        ContextRecord, DatasetDefinition, DatasetName, DatasetRecord, DatasetRepository,
        DatasetStatus, ResultRow,
    },
    environment::Environment,
    ports::RepositoryError,
};

const PRODUCTION: Environment = Environment::Production;
const NON_PRODUCTION: Environment = Environment::NonProduction;

struct Fixture {
    pool: PgPool,
    repository: PgDatasetRepository,
    definition: DatasetDefinition,
    study: Uuid,
    attachment: Uuid,
}

impl Fixture {
    async fn new() -> Self {
        let url = std::env::var("DATASET_TEST_DATABASE_URL")
            .expect("explicit disposable test database required");
        let pool = infra_pg::create_pool(&url).await.unwrap();
        infra_pg::run_migrations(&pool).await.unwrap();
        let catalog = infra_datasets::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../infra-datasets/tests/fixtures/catalog"
        )))
        .unwrap();
        let definition = catalog
            .datasets()
            .get(&DatasetName::parse("sample-outcomes").unwrap())
            .unwrap()
            .clone();
        let study = insert_study(&pool).await;
        let repository = PgDatasetRepository::new(pool.clone());
        repository.attach(study, &definition).await.unwrap();
        let attachment = attachment_id(&repository, study).await;
        Self {
            pool,
            repository,
            definition,
            study,
            attachment,
        }
    }

    fn name(&self) -> &DatasetName {
        self.definition.name()
    }

    async fn lock(&self, environment: Environment) -> RefreshLock {
        self.repository
            .try_lock(self.attachment, environment)
            .await
            .unwrap()
            .expect("lock is free")
    }

    async fn seed(&self, environment: Environment) -> Uuid {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO randomizations(id,study_id,unit_hash,arm,environment)
            VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(id)
        .bind(self.study)
        .bind(id.to_string())
        .bind(json!({"id": Uuid::from_u128(100), "name": "Treatment", "ratio": 1}))
        .bind(environment.as_str())
        .execute(&self.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO randomization_context VALUES ($1,'epic_csn',$2)")
            .bind(id)
            .bind(json!("captured-id"))
            .execute(&self.pool)
            .await
            .unwrap();
        id
    }

    async fn begin(&self, environment: Environment, definition: &DatasetDefinition) -> RefreshRun {
        self.lock(environment)
            .await
            .begin(definition)
            .await
            .unwrap()
    }

    async fn publish(&self, environment: Environment) -> Uuid {
        let mut run = self.begin(environment, &self.definition).await;
        stage_cohort(&mut run, &self.definition).await;
        let id = run.id();
        run.publish().await.unwrap();
        id
    }

    async fn snapshot_run(&self, environment: Environment) -> Option<Uuid> {
        self.repository
            .snapshot(self.study, self.name(), environment)
            .await
            .unwrap()
            .map(|snapshot| snapshot.run_id)
    }
}

async fn insert_study(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO studies(id,name,randomization_config) VALUES ($1,'Dataset test','{}')",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn attachment_id(repository: &PgDatasetRepository, study: Uuid) -> Uuid {
    let attachments = repository.list(study, PRODUCTION).await.unwrap();
    assert_eq!(attachments.len(), 1);
    attachments[0].id
}

fn record_for(definition: &DatasetDefinition, id: Uuid) -> DatasetRecord {
    let mut row: ResultRow = definition
        .fields()
        .iter()
        .map(|field| (field.column().as_ref().to_owned(), serde_json::Value::Null))
        .collect();
    row.insert("randomization_id".into(), json!(id));
    row.insert("LOS".into(), json!(4));
    let input = ContextRecord {
        randomization_id: id,
        randomization_time: "2026-10-01T14:30:00Z".parse().unwrap(),
        context: Default::default(),
    };
    definition
        .parse_results(&[input], vec![row])
        .unwrap()
        .remove(0)
}

async fn stage_cohort(run: &mut RefreshRun, definition: &DatasetDefinition) {
    loop {
        let page = run.next_page().await.unwrap();
        if page.is_empty() {
            return;
        }
        let records: Vec<_> = page
            .iter()
            .map(|record| record_for(definition, record.randomization_id))
            .collect();
        run.stage(&records).await.unwrap();
    }
}

fn revised(definition: &DatasetDefinition) -> DatasetDefinition {
    let mut value = serde_json::to_value(definition).unwrap();
    value["title"] = json!("Revised title");
    serde_json::from_value(value).unwrap()
}

/// Waits for the server to notice a dropped session and release its lock.
async fn lock_eventually(f: &Fixture, environment: Environment) -> RefreshLock {
    for _ in 0..50 {
        if let Some(lock) = f
            .repository
            .try_lock(f.attachment, environment)
            .await
            .unwrap()
        {
            return lock;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("lock was never released");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn snapshot_is_scoped_to_its_environment() {
    let f = Fixture::new().await;
    f.seed(PRODUCTION).await;
    f.seed(NON_PRODUCTION).await;

    let run = f.publish(PRODUCTION).await;

    assert_eq!(f.snapshot_run(PRODUCTION).await, Some(run));
    assert_eq!(f.snapshot_run(NON_PRODUCTION).await, None);
    let rows = f
        .repository
        .snapshot(f.study, f.name(), PRODUCTION)
        .await
        .unwrap()
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].arm_name, "Treatment");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn attachments_are_scoped_to_their_study() {
    let f = Fixture::new().await;
    let other = insert_study(&f.pool).await;

    let listed = f.repository.list(other, PRODUCTION).await.unwrap();

    assert!(listed.is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn snapshot_appears_only_when_run_is_published() {
    let f = Fixture::new().await;
    f.seed(PRODUCTION).await;
    let mut run = f.begin(PRODUCTION, &f.definition).await;
    stage_cohort(&mut run, &f.definition).await;
    assert_eq!(f.snapshot_run(PRODUCTION).await, None);
    let run_id = run.id();

    let row_count = run.publish().await.unwrap();

    assert_eq!(row_count, 1);
    assert_eq!(f.snapshot_run(PRODUCTION).await, Some(run_id));
    let attachment = &f.repository.list(f.study, PRODUCTION).await.unwrap()[0];
    assert_eq!(attachment.status, DatasetStatus::Succeeded);
    let published = attachment.snapshot.as_ref().expect("published snapshot");
    assert_eq!(published.row_count, 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn failed_run_keeps_last_published_snapshot() {
    let f = Fixture::new().await;
    f.seed(PRODUCTION).await;
    let published = f.publish(PRODUCTION).await;
    let run = f.begin(PRODUCTION, &f.definition).await;

    run.fail().await.unwrap();

    assert_eq!(f.snapshot_run(PRODUCTION).await, Some(published));
    let attachment = &f.repository.list(f.study, PRODUCTION).await.unwrap()[0];
    assert_eq!(attachment.status, DatasetStatus::Failed);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn abandoned_run_is_discarded_by_the_next_begin() {
    let f = Fixture::new().await;
    f.seed(PRODUCTION).await;
    let abandoned = f.begin(PRODUCTION, &f.definition).await.id();

    let replacement = lock_eventually(&f, PRODUCTION)
        .await
        .begin(&f.definition)
        .await
        .unwrap();

    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM dataset_runs WHERE id=$1")
        .bind(abandoned)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
    assert_ne!(replacement.id(), abandoned);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn detaching_during_a_run_makes_it_stale() {
    let f = Fixture::new().await;
    let randomization = f.seed(PRODUCTION).await;
    let mut run = f.begin(PRODUCTION, &f.definition).await;
    let run_id = run.id();
    f.repository.detach(f.study, f.name()).await.unwrap();

    let staged = run.stage(&[record_for(&f.definition, randomization)]).await;
    let published = run.publish().await;

    assert!(matches!(staged, Err(RefreshStoreError::StaleRun { run_id: id }) if id == run_id));
    assert!(matches!(published, Err(RefreshStoreError::StaleRun { run_id: id }) if id == run_id));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn run_started_before_detach_cannot_publish_after_reattach() {
    let f = Fixture::new().await;
    f.seed(PRODUCTION).await;
    let mut run = f.begin(PRODUCTION, &f.definition).await;
    stage_cohort(&mut run, &f.definition).await;
    f.repository.detach(f.study, f.name()).await.unwrap();
    f.repository.attach(f.study, &f.definition).await.unwrap();

    let published = run.publish().await;

    assert!(matches!(published, Err(RefreshStoreError::StaleRun { .. })));
    assert_eq!(f.snapshot_run(PRODUCTION).await, None);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn published_snapshot_keeps_the_definition_it_was_captured_with() {
    let f = Fixture::new().await;
    f.seed(PRODUCTION).await;
    f.publish(PRODUCTION).await;
    let changed = revised(&f.definition);
    assert_ne!(changed.fingerprint(), f.definition.fingerprint());

    let run = f.begin(PRODUCTION, &changed).await;

    let snapshot = f
        .repository
        .snapshot(f.study, f.name(), PRODUCTION)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        snapshot.definition.fingerprint(),
        f.definition.fingerprint()
    );
    run.fail().await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn attachment_title_comes_from_the_latest_published_definition() {
    let f = Fixture::new().await;
    f.seed(PRODUCTION).await;
    let before = f.repository.list(f.study, PRODUCTION).await.unwrap();
    let changed = revised(&f.definition);
    let mut run = f.begin(PRODUCTION, &changed).await;
    stage_cohort(&mut run, &changed).await;
    run.publish().await.unwrap();

    let after = f.repository.list(f.study, PRODUCTION).await.unwrap();

    assert_eq!(before[0].title, f.name().as_ref());
    assert_eq!(after[0].title, "Revised title");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn second_lock_attempt_returns_none_while_the_first_is_held() {
    let f = Fixture::new().await;
    let _held = f.lock(PRODUCTION).await;

    let second = f
        .repository
        .try_lock(f.attachment, PRODUCTION)
        .await
        .unwrap();

    assert!(second.is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn purging_a_randomization_removes_the_snapshot() {
    let f = Fixture::new().await;
    let randomization = f.seed(PRODUCTION).await;
    f.publish(PRODUCTION).await;

    sqlx::query("DELETE FROM randomizations WHERE id=$1")
        .bind(randomization)
        .execute(&f.pool)
        .await
        .unwrap();

    assert_eq!(f.snapshot_run(PRODUCTION).await, None);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn purging_one_environment_keeps_the_other_snapshot() {
    let f = Fixture::new().await;
    let production = f.seed(PRODUCTION).await;
    f.seed(NON_PRODUCTION).await;
    f.publish(PRODUCTION).await;
    f.publish(NON_PRODUCTION).await;

    sqlx::query("DELETE FROM randomizations WHERE id=$1")
        .bind(production)
        .execute(&f.pool)
        .await
        .unwrap();

    assert_eq!(f.snapshot_run(PRODUCTION).await, None);
    assert!(f.snapshot_run(NON_PRODUCTION).await.is_some());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn publishing_with_unstaged_rows_is_stale() {
    let f = Fixture::new().await;
    f.seed(PRODUCTION).await;
    let run = f.begin(PRODUCTION, &f.definition).await;

    let published = run.publish().await;

    assert!(matches!(published, Err(RefreshStoreError::StaleRun { .. })));
    assert_eq!(f.snapshot_run(PRODUCTION).await, None);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn staging_a_record_outside_the_cohort_is_rejected() {
    let f = Fixture::new().await;
    f.seed(PRODUCTION).await;
    let mut run = f.begin(PRODUCTION, &f.definition).await;
    let stranger = Uuid::now_v7();

    let staged = run.stage(&[record_for(&f.definition, stranger)]).await;

    assert!(matches!(
        staged,
        Err(RefreshStoreError::OutsideCohort { randomization_id, .. }) if randomization_id == stranger
    ));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn pages_return_the_captured_cohort_then_empty() {
    let f = Fixture::new().await;
    let randomization = f.seed(PRODUCTION).await;
    let mut run = f.begin(PRODUCTION, &f.definition).await;

    let first = run.next_page().await.unwrap();
    let second = run.next_page().await.unwrap();

    assert_eq!(first.len(), 1);
    assert_eq!(first[0].randomization_id, randomization);
    assert_eq!(first[0].context["epic_csn"], json!("captured-id"));
    assert!(second.is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn database_failures_expose_sqlstate_without_row_values() {
    let f = Fixture::new().await;

    let error = sqlx::query("SELECT 'sensitive-patient-value'::uuid")
        .execute(&f.pool)
        .await
        .unwrap_err();

    let failure = DatabaseFailure::from(&error);
    assert_eq!(failure.sqlstate(), Some("22P02"));
    assert!(!failure.to_string().contains("sensitive-patient-value"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn snapshot_with_undecodable_stored_definition_is_an_internal_error() {
    let f = Fixture::new().await;
    f.seed(PRODUCTION).await;
    f.publish(PRODUCTION).await;
    sqlx::query("UPDATE dataset_runs SET definition='{}'::jsonb WHERE attachment_id=$1")
        .bind(f.attachment)
        .execute(&f.pool)
        .await
        .unwrap();

    let result = f.repository.snapshot(f.study, f.name(), PRODUCTION).await;

    assert!(matches!(result, Err(RepositoryError::Internal(_))));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn published_run_stores_the_dataset_definition_without_a_report() {
    let f = Fixture::new().await;
    f.seed(PRODUCTION).await;
    f.publish(PRODUCTION).await;

    let (stored,): (serde_json::Value,) =
        sqlx::query_as("SELECT definition FROM dataset_runs WHERE attachment_id=$1")
            .bind(f.attachment)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let snapshot = f
        .repository
        .snapshot(f.study, f.name(), PRODUCTION)
        .await
        .unwrap()
        .unwrap();

    assert!(stored.get("report").is_none());
    assert_eq!(stored["name"], "sample-outcomes");
    assert_eq!(snapshot.fingerprint(), f.definition.fingerprint());
}
