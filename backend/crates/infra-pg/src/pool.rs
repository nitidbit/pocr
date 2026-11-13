use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Creates a connection pool with sensible defaults.
///
/// # Errors
///
/// Returns [`sqlx::Error`] if the URL is malformed or the initial connection
/// to PostgreSQL cannot be established.
pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
}

/// Runs embedded migrations against the pool.
///
/// # Errors
///
/// Returns [`sqlx::migrate::MigrateError`] if a migration fails to apply or the
/// recorded migration history diverges from the embedded set.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
