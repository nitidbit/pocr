use secrecy::SecretString;

use crate::error::{CaboodleError, CaboodleResult};

/// SQL Server service-account configuration for Caboodle.
#[derive(Debug, Clone)]
pub struct CaboodleConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: SecretString,
}

impl CaboodleConfig {
    /// Read required `CABOODLE_HOST`, `CABOODLE_DATABASE`, `CABOODLE_USERNAME`,
    /// and `CABOODLE_PASSWORD`. Only SQL Server authentication is supported.
    /// `CABOODLE_PORT` defaults to 1433 when absent.
    ///
    /// # Errors
    ///
    /// Return [`CaboodleError::Config`] for missing or blank required values,
    /// or an invalid or zero port.
    pub fn from_env() -> CaboodleResult<Self> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> CaboodleResult<Self> {
        let port = match lookup("CABOODLE_PORT") {
            None => 1433,
            Some(value) => value
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or_else(|| {
                    CaboodleError::Config("CABOODLE_PORT must be between 1 and 65535".into())
                })?,
        };
        Ok(Self {
            host: require(&lookup, "CABOODLE_HOST")?,
            port,
            database: require(&lookup, "CABOODLE_DATABASE")?,
            username: require(&lookup, "CABOODLE_USERNAME")?,
            password: SecretString::from(require(&lookup, "CABOODLE_PASSWORD")?),
        })
    }
}

fn require(lookup: impl Fn(&str) -> Option<String>, name: &str) -> CaboodleResult<String> {
    lookup(name)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CaboodleError::Config(format!("missing or empty {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    fn config(overrides: &[(&str, Option<&str>)]) -> CaboodleResult<CaboodleConfig> {
        CaboodleConfig::from_lookup(|name| {
            if let Some((_, value)) = overrides.iter().find(|(key, _)| *key == name) {
                return value.map(str::to_owned);
            }
            match name {
                "CABOODLE_HOST" => Some("caboodle.example".into()),
                "CABOODLE_DATABASE" => Some("CDW".into()),
                "CABOODLE_USERNAME" => Some("svc_pocr".into()),
                "CABOODLE_PASSWORD" => Some("secret-value".into()),
                _ => None,
            }
        })
    }

    #[test]
    fn service_account_credentials_are_preserved_and_password_is_redacted() {
        let config = config(&[]).unwrap();
        assert_eq!(config.username, "svc_pocr");
        assert_eq!(config.password.expose_secret(), "secret-value");
        assert_eq!(config.port, 1433);
        assert!(!format!("{config:?}").contains("secret-value"));
    }

    #[test]
    fn required_values_must_be_present_and_nonempty() {
        for name in [
            "CABOODLE_HOST",
            "CABOODLE_DATABASE",
            "CABOODLE_USERNAME",
            "CABOODLE_PASSWORD",
        ] {
            for value in [None, Some(""), Some("   ")] {
                assert!(
                    config(&[(name, value)]).is_err(),
                    "accepted {name}={value:?}"
                );
            }
        }
    }

    #[test]
    fn explicit_port_must_be_valid_and_nonzero() {
        assert_eq!(
            config(&[("CABOODLE_PORT", Some("1444"))]).unwrap().port,
            1444
        );
        for value in ["", "0", "invalid", "65536"] {
            assert!(config(&[("CABOODLE_PORT", Some(value))]).is_err());
        }
    }

    #[test]
    fn password_whitespace_is_preserved() {
        let config = config(&[("CABOODLE_PASSWORD", Some(" secret "))]).unwrap();
        assert_eq!(config.password.expose_secret(), " secret ");
    }
}
