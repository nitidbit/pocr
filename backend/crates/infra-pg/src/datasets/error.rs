//! Errors raised by dataset persistence and refresh sessions.

use std::borrow::Cow;

use uuid::Uuid;

/// Sanitized category of a database failure.
///
/// Carries the SQLSTATE or failure class only. Driver messages can contain
/// row values and are never retained.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(transparent)]
pub struct DatabaseFailure(FailureKind);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
enum FailureKind {
    #[error("database error (SQLSTATE {0})")]
    Sqlstate(String),
    #[error("database pool timed out")]
    PoolTimeout,
    #[error("database pool closed")]
    PoolClosed,
    #[error("database connection failed")]
    Connection,
    #[error("database value could not be decoded")]
    Decode,
    #[error("database operation failed")]
    Other,
}

impl DatabaseFailure {
    /// SQLSTATE reported by the server, if the failure came from the database.
    #[must_use]
    pub fn sqlstate(&self) -> Option<&str> {
        match &self.0 {
            FailureKind::Sqlstate(code) => Some(code),
            FailureKind::PoolTimeout
            | FailureKind::PoolClosed
            | FailureKind::Connection
            | FailureKind::Decode
            | FailureKind::Other => None,
        }
    }
}

impl From<&sqlx::Error> for DatabaseFailure {
    fn from(error: &sqlx::Error) -> Self {
        Self(match error {
            sqlx::Error::Database(error) => FailureKind::Sqlstate(
                error
                    .code()
                    .map_or_else(|| "unknown".to_owned(), Cow::into_owned),
            ),
            sqlx::Error::PoolTimedOut => FailureKind::PoolTimeout,
            sqlx::Error::PoolClosed => FailureKind::PoolClosed,
            sqlx::Error::Io(_) | sqlx::Error::Tls(_) => FailureKind::Connection,
            sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_) => FailureKind::Decode,
            _ => FailureKind::Other,
        })
    }
}

/// Reasons a dataset persistence or refresh operation fails.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RefreshStoreError {
    /// A database statement failed.
    #[error("{operation} failed: {cause}")]
    Database {
        /// Operation being performed, for example `"stage results"`.
        operation: &'static str,
        /// Sanitized failure category.
        cause: DatabaseFailure,
    },
    /// The run was detached, purged, superseded, or already finished.
    #[error("run {run_id} is no longer current")]
    StaleRun { run_id: Uuid },
    /// A staged record is not an unstaged row of the run's captured cohort.
    #[error("randomization {randomization_id} is not an unstaged row of run {run_id}")]
    OutsideCohort {
        run_id: Uuid,
        randomization_id: Uuid,
    },
    /// Stored dataset definition of a run cannot be decoded.
    #[error("stored definition for run {run_id} cannot be decoded")]
    CorruptRun {
        run_id: Uuid,
        /// Why the stored definition was rejected.
        #[source]
        source: serde_json::Error,
    },
    /// An attachment holds a dataset name that is not valid.
    #[error("attachment {attachment_id} has an invalid dataset name")]
    CorruptTarget { attachment_id: Uuid },
}

impl RefreshStoreError {
    /// Wrap a driver error, keeping only its sanitized category.
    pub(crate) fn database(operation: &'static str, error: &sqlx::Error) -> Self {
        Self::Database {
            operation,
            cause: DatabaseFailure::from(error),
        }
    }
}
