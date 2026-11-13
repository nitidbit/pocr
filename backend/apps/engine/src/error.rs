//! Startup and serving errors of the engine binary.

use std::io;
use std::net::{AddrParseError, SocketAddr};
use std::path::PathBuf;

use thiserror::Error;

/// Result type for engine startup and serving.
pub type EngineResult<T> = Result<T, EngineError>;

/// Failures that stop the engine from starting or serving.
#[derive(Debug, Error)]
pub enum EngineError {
    /// `HOST` and `API_PORT` do not form a socket address.
    #[error("invalid listen address {address}")]
    InvalidAddress {
        /// The `host:port` string that failed to parse.
        address: String,
        /// Underlying parse failure.
        #[source]
        source: AddrParseError,
    },
    /// A `<var>_FILE` secret file could not be read.
    #[error("failed to read {variable}_FILE at {}", path.display())]
    SecretFile {
        /// Variable whose `_FILE` counterpart was set.
        variable: String,
        /// Path named by the variable.
        path: PathBuf,
        /// Underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// Connecting to PostgreSQL failed.
    #[error("failed to connect to database")]
    Database(#[source] sqlx::Error),
    /// Applying PostgreSQL migrations failed.
    #[error("failed to run migrations")]
    Migrations(#[source] sqlx::migrate::MigrateError),
    /// Configuring the Redis client failed.
    #[error("failed to configure redis client")]
    Redis(#[source] redis::RedisError),
    /// Loading the dataset catalog failed.
    #[error("failed to load dataset catalog")]
    Catalog(#[source] infra_datasets::LoadError),
    /// Binding the listen socket failed.
    #[error("failed to bind {addr}")]
    Bind {
        /// Address that could not be bound.
        addr: SocketAddr,
        /// Underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// The HTTP server stopped with an I/O error.
    #[error("server failed")]
    Serve(#[source] io::Error),
}
