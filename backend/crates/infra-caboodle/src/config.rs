use secrecy::SecretString;

/// Configuration for connecting to the Caboodle MS-SQL instance.
#[derive(Debug, Clone)]
pub struct CaboodleConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: SecretString,
}

impl CaboodleConfig {
    /// Build config from environment variables.
    ///
    /// # Errors
    ///
    /// Return `VarError` if any required variable (`CABOODLE_HOST`,
    /// `CABOODLE_DATABASE`, `CABOODLE_USERNAME`, `CABOODLE_PASSWORD`)
    /// is missing. `CABOODLE_PORT` defaults to 1433 if absent or
    /// unparseable.
    pub fn from_env() -> Result<Self, std::env::VarError> {
        Ok(Self {
            host: std::env::var("CABOODLE_HOST")?,
            port: std::env::var("CABOODLE_PORT")
                .unwrap_or_else(|_| "1433".to_string())
                .parse()
                .unwrap_or(1433),
            database: std::env::var("CABOODLE_DATABASE")?,
            username: std::env::var("CABOODLE_USERNAME")?,
            password: SecretString::from(std::env::var("CABOODLE_PASSWORD")?),
        })
    }
}
