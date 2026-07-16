//! `pocr-migrate` — one-shot ETL migrating a live study from the legacy POC
//! engine's Redis to the POCR PostgreSQL + Redis stores.
//!
//! See `docs/superpowers/specs/2026-07-15-live-study-redis-to-postgres-migration-design.md`.

mod ids;
mod legacy;
mod transform;

use std::process::ExitCode;

use tracing::error;

#[tokio::main]
async fn main() -> ExitCode {
    let _guard = observability::init(observability::Config {
        default_filter: "info,sqlx=warn",
        ..observability::Config::for_service("pocr-migrate")
    });

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!(error = %e, "migration failed");
            ExitCode::FAILURE
        }
    }
}

/// Errors from the migration pipeline.
#[derive(Debug, thiserror::Error)]
enum MigrateError {
    /// The migration orchestration is not yet wired.
    #[error("not yet implemented")]
    NotImplemented,
}

async fn run() -> Result<(), MigrateError> {
    Err(MigrateError::NotImplemented)
}
