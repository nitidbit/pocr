use pocr_core::ports::RepositoryError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PgError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

impl From<PgError> for RepositoryError {
    fn from(e: PgError) -> Self {
        match e {
            PgError::NotFound => RepositoryError::NotFound,
            PgError::Conflict(msg) => RepositoryError::Conflict(msg),
            PgError::Database(e) => RepositoryError::Internal(e.to_string()),
            PgError::Migration(e) => RepositoryError::Internal(e.to_string()),
        }
    }
}
