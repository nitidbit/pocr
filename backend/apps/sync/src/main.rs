mod datasets;
use std::collections::HashMap;
use std::env;
use std::num::{NonZeroU64, NonZeroUsize};
use std::process::ExitCode;
use std::time::Duration;

use chrono::{DateTime, Utc};
use pocr_core::demographics::PatientDemographics;
use pocr_core::ports::DemographicsRepository;
use tracing::{error, info};

/// Time limit for one dataset procedure call unless `DATASET_PROCEDURE_TIMEOUT_SECS` is set.
const DEFAULT_PROCEDURE_TIMEOUT: Duration = Duration::from_mins(5);

#[tokio::main]
async fn main() -> ExitCode {
    let _guard = observability::init(observability::Config {
        default_filter: "info,sqlx=warn",
        ..observability::Config::for_service("pocr-sync")
    });

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!(error = %e, "sync failed");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), SyncError> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pocr:pocr@localhost:5432/pocr".into());
    let pg_pool = infra_pg::create_pool(&database_url).await?;
    infra_pg::run_migrations(&pg_pool).await?;
    info!("connected to PostgreSQL");

    let catalog = infra_datasets::from_env().map_err(|e| SyncError::Config(e.display_chain()))?;
    let dataset_result = match catalog {
        Some(catalog) => refresh_datasets(&pg_pool, catalog.datasets()).await,
        None => Ok(()),
    };
    let demographic_result = sync_demographics(pg_pool).await;
    dataset_result?;
    demographic_result
}

async fn refresh_datasets(
    pg_pool: &sqlx::PgPool,
    registry: &pocr_core::dataset::DatasetRegistry,
) -> Result<(), SyncError> {
    let environment = env::var("DATASET_ENVIRONMENT")
        .ok()
        .and_then(|s| pocr_core::environment::Environment::parse(&s))
        .ok_or_else(|| {
            SyncError::Config("DATASET_ENVIRONMENT must be production or nonproduction".into())
        })?;
    let caboodle = infra_caboodle::CaboodleConfig::from_env()
        .map_err(|e| SyncError::Config(format!("missing Caboodle env vars: {e}")))?;
    let concurrency = match env::var("DATASET_REFRESH_CONCURRENCY") {
        Ok(raw) => raw.parse::<NonZeroUsize>().map_err(|e| {
            SyncError::Config(format!(
                "DATASET_REFRESH_CONCURRENCY must be a positive integer: {e}"
            ))
        })?,
        Err(_) => NonZeroUsize::MIN,
    };
    let procedure_timeout = match env::var("DATASET_PROCEDURE_TIMEOUT_SECS") {
        Ok(raw) => raw
            .parse::<NonZeroU64>()
            .map(|secs| Duration::from_secs(secs.get()))
            .map_err(|e| {
                SyncError::Config(format!(
                    "DATASET_PROCEDURE_TIMEOUT_SECS must be a positive integer: {e}"
                ))
            })?,
        Err(_) => DEFAULT_PROCEDURE_TIMEOUT,
    };
    let settings = datasets::RefreshSettings {
        environment,
        concurrency,
        procedure_timeout,
    };
    let repository = infra_pg::PgDatasetRepository::new(pg_pool.clone());
    let summary = datasets::refresh_all(&repository, registry, &caboodle, settings).await?;
    match summary.failed {
        0 => Ok(()),
        failed => Err(SyncError::DatasetRefresh { failed }),
    }
}

async fn sync_demographics(pg_pool: sqlx::PgPool) -> Result<(), SyncError> {
    let caboodle_config = infra_caboodle::CaboodleConfig::from_env()
        .map_err(|e| SyncError::Config(format!("missing Caboodle env vars: {e}")))?;
    let mut caboodle = infra_caboodle::CaboodleClient::connect(&caboodle_config).await?;
    info!("connected to Caboodle");

    let patients: Vec<PatientRow> = sqlx::query_as(include_str!("patients.sql"))
        .fetch_all(&pg_pool)
        .await?;

    if patients.is_empty() {
        info!("no randomized patients with Epic IDs found, nothing to sync");
        return Ok(());
    }

    info!(
        patient_count = patients.len(),
        "found randomized patients to sync"
    );

    let patient_ids: Vec<String> = patients.iter().map(|p| p.epic_patient_id.clone()).collect();

    let demographics =
        infra_caboodle::queries::fetch_demographics(&mut caboodle, &patient_ids).await?;
    info!(
        found = demographics.len(),
        total = patient_ids.len(),
        "fetched demographics from Caboodle"
    );

    if demographics.is_empty() {
        info!("no matching patients in Caboodle, nothing to upsert");
        return Ok(());
    }

    let not_found_count = patient_ids.len().saturating_sub(demographics.len());

    // Build lookup by patient_epic_id for BMI query filtering
    let demo_by_id: HashMap<&str, &infra_caboodle::CaboodleDemographics> = demographics
        .iter()
        .map(|d| (d.patient_epic_id.as_str(), d))
        .collect();

    // Collect patient_keys for secondary queries
    let patient_keys: Vec<i64> = demographics.iter().map(|d| d.patient_key).collect();

    let races = infra_caboodle::queries::fetch_races(&mut caboodle, &patient_keys).await?;
    let insurance = infra_caboodle::queries::fetch_insurance(&mut caboodle, &patient_keys).await?;

    // BMI needs (epic_id, randomization_date) pairs for patients found in Caboodle
    let bmi_patients: Vec<(String, DateTime<Utc>)> = patients
        .iter()
        .filter(|p| demo_by_id.contains_key(p.epic_patient_id.as_str()))
        .map(|p| (p.epic_patient_id.clone(), p.issued_at))
        .collect();
    let bmi = infra_caboodle::queries::fetch_bmi(&mut caboodle, &bmi_patients).await?;

    let now = Utc::now();
    let mut batch: Vec<PatientDemographics> = Vec::with_capacity(demographics.len());

    for demo in &demographics {
        let patient_races = races.get(&demo.patient_key).cloned();
        let patient_insurance = insurance.get(&demo.patient_key).cloned();
        let patient_bmi = bmi.get(&demo.patient_epic_id).copied();

        batch.push(PatientDemographics::new(
            demo.patient_epic_id.clone(),
            demo.birth_date,
            demo.sex.clone(),
            patient_races,
            demo.ethnicity.clone(),
            demo.primary_language.clone(),
            demo.smoking_status.clone(),
            patient_insurance,
            patient_bmi,
            now,
        ));
    }

    let repo = infra_pg::PgDemographicsRepository::new(pg_pool);
    repo.upsert_batch(&batch).await?;

    info!(
        synced = batch.len(),
        not_found = not_found_count,
        "sync complete"
    );

    Ok(())
}

/// Row type for the patient query.
#[derive(sqlx::FromRow)]
struct PatientRow {
    epic_patient_id: String,
    issued_at: DateTime<Utc>,
}

/// Errors from the sync pipeline.
#[derive(Debug, thiserror::Error)]
enum SyncError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("PostgreSQL error: {0}")]
    Postgres(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Caboodle error: {0}")]
    Caboodle(#[from] infra_caboodle::CaboodleError),

    #[error("demographics upsert error: {0}")]
    Upsert(#[from] pocr_core::ports::RepositoryError),

    #[error("dataset store error: {0}")]
    DatasetStore(#[from] infra_pg::datasets::RefreshStoreError),

    #[error("{failed} dataset refreshes failed")]
    DatasetRefresh { failed: usize },
}
