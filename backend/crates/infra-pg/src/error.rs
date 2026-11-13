use pocr_core::ports::RepositoryError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PgError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("migration error")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

impl From<PgError> for RepositoryError {
    fn from(e: PgError) -> Self {
        match e {
            PgError::NotFound => RepositoryError::NotFound,
            PgError::Conflict(msg) => RepositoryError::Conflict(msg),
            PgError::Database(e) => RepositoryError::storage(e),
            PgError::Migration(e) => RepositoryError::storage(e),
        }
    }
}
