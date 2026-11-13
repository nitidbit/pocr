use std::fmt;
use std::time::Duration;

use secrecy::ExposeSecret;
use tiberius::{AuthMethod, Client, Config as TiberiusConfig, EncryptionLevel, Query};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;
use tracing::{info, instrument};

use crate::config::CaboodleConfig;
use crate::error::{CaboodleError, CaboodleResult, DriverFailure};

/// Connected client for the Caboodle MS-SQL database.
pub struct CaboodleClient {
    pub(crate) client: Client<tokio_util::compat::Compat<TcpStream>>,
}

impl fmt::Debug for CaboodleClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CaboodleClient").finish_non_exhaustive()
    }
}

impl CaboodleClient {
    /// Establishes a connection to Caboodle using SQL Server authentication.
    ///
    /// # Errors
    ///
    /// Returns [`CaboodleError::TcpConnect`], [`CaboodleError::Socket`], or
    /// [`CaboodleError::Handshake`] if connecting fails, and
    /// [`CaboodleError::ConnectTimeout`] if it exceeds 15 seconds.
    #[instrument(skip(config), fields(host = %config.host, port = config.port, database = %config.database))]
    pub async fn connect(config: &CaboodleConfig) -> CaboodleResult<Self> {
        let mut tib_config = TiberiusConfig::new();
        tib_config.host(&config.host);
        tib_config.port(config.port);
        tib_config.database(&config.database);
        tib_config.authentication(AuthMethod::sql_server(
            &config.username,
            config.password.expose_secret(),
        ));
        tib_config.encryption(EncryptionLevel::Required);

        let client = tokio::time::timeout(Duration::from_secs(15), async {
            let tcp = TcpStream::connect(tib_config.get_addr())
                .await
                .map_err(CaboodleError::TcpConnect)?;

            tcp.set_nodelay(true).map_err(CaboodleError::Socket)?;

            let client = Client::connect(tib_config, tcp.compat_write())
                .await
                .map_err(|e| CaboodleError::Handshake(DriverFailure::from(&e)))?;

            Ok::<_, CaboodleError>(client)
        })
        .await
        .map_err(|_elapsed| CaboodleError::ConnectTimeout)??;

        info!("connected to Caboodle");
        Ok(Self { client })
    }

    /// Round-trips a trivial query to confirm the connection is live.
    ///
    /// Intended for connectivity smoke tests, not the sync pipeline itself.
    /// Consumes the connection so a timed-out protocol stream cannot be reused.
    ///
    /// # Errors
    ///
    /// Returns [`CaboodleError::Query`] if the query fails and
    /// [`CaboodleError::QueryTimeout`] if it exceeds 15 seconds.
    pub async fn ping(mut self) -> CaboodleResult<()> {
        tokio::time::timeout(Duration::from_secs(15), async {
            Query::new("SELECT 1")
                .query(&mut self.client)
                .await
                .map_err(|e| CaboodleError::query("ping", &e))?
                .into_first_result()
                .await
                .map_err(|e| CaboodleError::query("ping results", &e))?;
            Ok(())
        })
        .await
        .map_err(|_elapsed| CaboodleError::QueryTimeout { operation: "ping" })?
    }
}
