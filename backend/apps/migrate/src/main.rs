//! `pocr-migrate` — one-shot ETL migrating a live study from the legacy POC
//! engine's Redis to the POCR PostgreSQL + Redis stores.
//!
//! Reads the legacy Redis directly, transforms records in typed Rust against
//! the real `pocr-core` models, and writes new Postgres rows + Redis block
//! state. Re-runnable and idempotent via a persisted `migration-manifest.json`.
//!
//! Configuration (environment variables):
//! - `LEGACY_REDIS_URL` — source Redis (read). Required.
//! - `DATABASE_URL` — POCR Postgres (write). Defaults to the local dev URL.
//! - `POCR_REDIS_URL` — POCR Redis block cache (write). Required.
//! - `MIGRATION_MANIFEST` — manifest path. Defaults to `migration-manifest.json`.
//!
//! See `docs/superpowers/specs/2026-07-15-live-study-redis-to-postgres-migration-design.md`.

mod ids;
mod legacy;
mod migrate;
mod sink;
mod source;
mod transform;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use tracing::{error, info};

use infra_redis::RedisPool;

use crate::migrate::{load_manifest, migrate, save_manifest};
use crate::sink::PocrSink;
use crate::source::LegacySource;

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

/// Errors from the migration binary.
#[derive(Debug, thiserror::Error)]
enum RunError {
    /// A required configuration value was missing.
    #[error("missing required env var: {0}")]
    MissingEnv(&'static str),
    /// Connecting to the legacy or POCR Redis failed.
    #[error("redis connection error: {0}")]
    Redis(#[from] redis::RedisError),
    /// Connecting to or migrating Postgres failed.
    #[error("postgres error: {0}")]
    Postgres(#[from] sqlx::Error),
    /// Running Postgres migrations failed.
    #[error("migration error: {0}")]
    PgMigrate(#[from] sqlx::migrate::MigrateError),
    /// The migration itself failed.
    #[error(transparent)]
    Migrate(#[from] migrate::MigrateError),
}

fn required_env(key: &'static str) -> Result<String, RunError> {
    env::var(key).map_err(|_| RunError::MissingEnv(key))
}

async fn run() -> Result<(), RunError> {
    let legacy_redis_url = required_env("LEGACY_REDIS_URL")?;
    let pocr_redis_url = required_env("POCR_REDIS_URL")?;
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pocr:pocr@localhost:5432/pocr".into());
    let manifest_path = PathBuf::from(
        env::var("MIGRATION_MANIFEST").unwrap_or_else(|_| "migration-manifest.json".into()),
    );

    // Source: legacy Redis (read-only).
    let legacy_pool = RedisPool::new(&legacy_redis_url)?;
    let source = LegacySource::new(legacy_pool);
    info!("connected to legacy Redis");

    // Sink: POCR Postgres + Redis.
    let pg_pool = infra_pg::create_pool(&database_url).await?;
    infra_pg::run_migrations(&pg_pool).await?;
    let pocr_redis = Arc::new(RedisPool::new(&pocr_redis_url)?);
    let sink = PocrSink::new(pg_pool, pocr_redis);
    info!("connected to POCR Postgres and Redis");

    // Load-or-init the manifest (idempotency), migrate, persist the manifest.
    let mut manifest = load_manifest(&manifest_path)?;
    let counts = migrate(&source, &sink, &mut manifest).await?;
    save_manifest(&manifest_path, &manifest)?;

    info!(
        studies = counts.studies,
        randomizations = counts.randomizations,
        locators = counts.locators,
        blocks = counts.blocks,
        manifest = %manifest_path.display(),
        "migration complete"
    );
    Ok(())
}
