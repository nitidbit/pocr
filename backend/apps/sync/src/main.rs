//! The `pocr-sync` binary: one-shot dataset refresh into PostgreSQL.

mod datasets;

use std::env;
use std::num::{NonZeroU64, NonZeroUsize};
use std::process::ExitCode;
use std::time::Duration;

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
            error!(error = %format!("{:#}", anyhow::Error::from(e)), "sync failed");
            ExitCode::FAILURE
        }
    }
}

/// Refreshes every dataset in the configured catalog.
///
/// # Errors
///
/// Returns [`SyncError::CatalogUnset`] when no catalog is configured.
async fn run() -> Result<(), SyncError> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pocr:pocr@localhost:5432/pocr".into());
    let pg_pool = infra_pg::create_pool(&database_url).await?;
    infra_pg::run_migrations(&pg_pool).await?;
    info!("connected to PostgreSQL");

    let catalog = infra_datasets::from_env()
        .map_err(SyncError::Catalog)?
        .ok_or(SyncError::CatalogUnset)?;
    refresh_datasets(&pg_pool, catalog.datasets()).await
}

/// Refreshes every dataset attachment using settings read from the environment.
async fn refresh_datasets(
    pg_pool: &sqlx::PgPool,
    registry: &pocr_core::dataset::DatasetRegistry,
) -> Result<(), SyncError> {
    let environment = env::var("DATASET_ENVIRONMENT")
        .ok()
        .and_then(|s| pocr_core::environment::Environment::parse(&s))
        .ok_or(SyncError::InvalidEnvironment)?;
    let caboodle = infra_caboodle::CaboodleConfig::from_env().map_err(SyncError::CaboodleConfig)?;
    let concurrency = match env::var("DATASET_REFRESH_CONCURRENCY") {
        Ok(raw) => raw
            .parse::<NonZeroUsize>()
            .map_err(|source| SyncError::InvalidSetting {
                variable: "DATASET_REFRESH_CONCURRENCY",
                source,
            })?,
        Err(_) => NonZeroUsize::MIN,
    };
    let procedure_timeout = match env::var("DATASET_PROCEDURE_TIMEOUT_SECS") {
        Ok(raw) => raw
            .parse::<NonZeroU64>()
            .map(|secs| Duration::from_secs(secs.get()))
            .map_err(|source| SyncError::InvalidSetting {
                variable: "DATASET_PROCEDURE_TIMEOUT_SECS",
                source,
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

/// Errors from the sync pipeline.
#[derive(Debug, thiserror::Error)]
enum SyncError {
    /// The dataset catalog could not be loaded.
    #[error("failed to load dataset catalog")]
    Catalog(#[source] infra_datasets::LoadError),

    /// `DATASETS_DIR` is unset, so there is no catalog to refresh.
    #[error("DATASETS_DIR is not set; sync has nothing to refresh")]
    CatalogUnset,

    /// `DATASET_ENVIRONMENT` is unset or not a known environment.
    #[error("DATASET_ENVIRONMENT must be production or nonproduction")]
    InvalidEnvironment,

    /// A numeric refresh setting is not a positive integer.
    #[error("{variable} must be a positive integer")]
    InvalidSetting {
        /// Name of the offending variable.
        variable: &'static str,
        /// The parse failure.
        #[source]
        source: std::num::ParseIntError,
    },

    /// Caboodle connection settings are missing or invalid.
    #[error("missing or invalid Caboodle environment variables")]
    CaboodleConfig(#[source] infra_caboodle::CaboodleError),

    /// A PostgreSQL query or connection failed.
    #[error("PostgreSQL error")]
    Postgres(#[from] sqlx::Error),

    /// Applying PostgreSQL migrations failed.
    #[error("migration error")]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// A Caboodle query or connection failed.
    #[error("Caboodle error")]
    Caboodle(#[from] infra_caboodle::CaboodleError),

    /// The dataset store could not list attachments.
    #[error("dataset store error")]
    DatasetStore(#[from] infra_pg::RefreshStoreError),

    /// One or more attachments failed to refresh.
    #[error("{failed} dataset refreshes failed")]
    DatasetRefresh {
        /// Number of failed attachments.
        failed: usize,
    },
}
