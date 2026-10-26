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

    /// Missing or conflicting connection configuration.
    #[error("configuration error: {0}")]
    Config(String),
}

/// Preserve diagnostic categories/codes without retaining server messages or data.
pub(crate) fn driver_error(error: &tiberius::error::Error) -> String {
    if let Some(code) = error.code() {
        return format!("SQL Server error {code}");
    }
    match error {
        tiberius::error::Error::Io { kind, .. } => format!("I/O error ({kind:?})"),
        tiberius::error::Error::Tls(_) => "TLS handshake or certificate validation failed".into(),
        tiberius::error::Error::Conversion(_) => "unexpected column type".into(),
        _ => "database protocol or data decoding error".into(),
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
            assert!(!driver_error(&error).contains("sensitive-patient-value"));
        }
    }
}
