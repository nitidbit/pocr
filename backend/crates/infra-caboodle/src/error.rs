use thiserror::Error;

/// Alias for results within this crate.
pub type CaboodleResult<T> = Result<T, CaboodleError>;

/// Errors arising from Caboodle (MS-SQL) operations.
#[derive(Debug, Error)]
pub enum CaboodleError {
    /// TCP or TDS-level connection failure.
    #[error("connection failed: {0}")]
    Connection(String),

    /// Query execution or result-set read failure.
    #[error("query failed: {0}")]
    Query(String),

    /// Row data could not be converted to the expected Rust type.
    #[error("data conversion error: {0}")]
    Conversion(String),
}
