use thiserror::Error;

/// Alias for results within this crate.
pub type CaboodleResult<T> = Result<T, CaboodleError>;

/// Errors arising from Caboodle (MS-SQL) operations.
#[derive(Debug, Error)]
pub enum CaboodleError {
    /// The TCP connection to the server could not be opened.
    #[error("TCP connect failed")]
    TcpConnect(#[source] std::io::Error),

    /// The socket could not be configured after connecting.
    #[error("socket configuration failed")]
    Socket(#[source] std::io::Error),

    /// The TDS handshake or login failed.
    #[error("TDS connect failed")]
    Handshake(#[source] DriverFailure),

    /// Connecting did not finish within its time limit.
    #[error("connection timed out after 15 seconds")]
    ConnectTimeout,

    /// A query could not be sent, or its results could not be read.
    #[error("{operation} failed")]
    Query {
        /// Operation being performed, for example `"fetch_races"`.
        operation: &'static str,
        /// Sanitized failure category.
        #[source]
        cause: DriverFailure,
    },

    /// A query did not finish within its time limit.
    #[error("{operation} timed out after 15 seconds")]
    QueryTimeout {
        /// Operation being performed, for example `"ping"`.
        operation: &'static str,
    },

    /// Row data could not be converted to the expected Rust type.
    #[error("data conversion error: {0}")]
    Conversion(String),

    /// Missing or conflicting connection configuration.
    #[error("configuration error: {0}")]
    Config(String),

    /// A dataset procedure did not finish within its time limit.
    #[error("dataset execution timed out")]
    Timeout,

    /// A dataset procedure returned zero or several result sets instead of one.
    #[error("dataset procedure returned {actual} result sets, expected one")]
    ResultSetCount {
        /// Number of result sets the procedure returned.
        actual: usize,
    },

    /// A dataset procedure returned a result that violates the dataset contract.
    #[error("dataset result contract violated")]
    Contract(#[from] pocr_core::dataset::ResultsError),
}

impl CaboodleError {
    /// Wraps a driver failure raised while running `operation`.
    pub(crate) fn query(operation: &'static str, error: &tiberius::error::Error) -> Self {
        Self::Query {
            operation,
            cause: DriverFailure::from(error),
        }
    }
}

/// Sanitized category of a SQL Server driver failure.
///
/// Carries the server error code or failure class only. Driver messages can
/// contain row values and are never retained.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(transparent)]
pub struct DriverFailure(FailureKind);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
enum FailureKind {
    #[error("SQL Server error {0}")]
    Server(u32),
    #[error("I/O error ({0:?})")]
    Io(std::io::ErrorKind),
    #[error("TLS handshake or certificate validation failed")]
    Tls,
    #[error("unexpected column type")]
    Conversion,
    #[error("database protocol or data decoding error")]
    Other,
}

impl From<&tiberius::error::Error> for DriverFailure {
    fn from(error: &tiberius::error::Error) -> Self {
        use tiberius::error::Error;
        if let Some(code) = error.code() {
            return Self(FailureKind::Server(code));
        }
        Self(match error {
            Error::Io { kind, .. } => FailureKind::Io(*kind),
            Error::Tls(_) => FailureKind::Tls,
            Error::Conversion(_) => FailureKind::Conversion,
            _ => FailureKind::Other,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_messages_cannot_leak_sensitive_values() {
        for error in [
            tiberius::error::Error::Conversion("sensitive-patient-value".into()),
            tiberius::error::Error::Protocol("sensitive-patient-value".into()),
            tiberius::error::Error::Tls("sensitive-patient-value".into()),
            tiberius::error::Error::Io {
                kind: std::io::ErrorKind::ConnectionReset,
                message: "sensitive-patient-value".into(),
            },
        ] {
            assert!(
                !DriverFailure::from(&error)
                    .to_string()
                    .contains("sensitive-patient-value")
            );
        }
    }
}
