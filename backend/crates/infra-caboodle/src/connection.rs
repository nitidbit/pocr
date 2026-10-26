use std::time::Duration;

use secrecy::ExposeSecret;
use tiberius::{AuthMethod, Client, Config as TiberiusConfig, EncryptionLevel, Query};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;
use tracing::{info, instrument};

use crate::config::CaboodleConfig;
use crate::error::{CaboodleError, CaboodleResult, driver_error};

/// A connected client to the Caboodle MS-SQL database.
pub struct CaboodleClient {
    pub(crate) client: Client<tokio_util::compat::Compat<TcpStream>>,
}

impl CaboodleClient {
    /// Establish a connection to Caboodle using SQL Server authentication.
    ///
    /// # Errors
    ///
    /// Return [`CaboodleError::Connection`] if the TCP connection, socket
    /// configuration, or TDS handshake fails.
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
                .map_err(|e| CaboodleError::Connection(format!("TCP connect failed: {e}")))?;

            tcp.set_nodelay(true)
                .map_err(|e| CaboodleError::Connection(format!("set_nodelay failed: {e}")))?;

            let client = Client::connect(tib_config, tcp.compat_write())
                .await
                .map_err(|e| {
                    CaboodleError::Connection(format!("TDS connect failed: {}", driver_error(&e)))
                })?;

            Ok::<_, CaboodleError>(client)
        })
        .await
        .map_err(|_| CaboodleError::Connection("connection timed out after 15 seconds".into()))??;

        info!("connected to Caboodle");
        Ok(Self { client })
    }

    /// Round-trip a trivial query to confirm the connection is live.
    ///
    /// Intended for connectivity smoke tests, not the sync pipeline itself.
    ///
    /// # Errors
    ///
    /// Return [`CaboodleError::Query`] if the query fails or exceeds 15 seconds.
    /// The connection is consumed so a timed-out protocol stream cannot be reused.
    pub async fn ping(mut self) -> CaboodleResult<()> {
        tokio::time::timeout(Duration::from_secs(15), async {
            Query::new("SELECT 1")
                .query(&mut self.client)
                .await
                .map_err(|e| CaboodleError::Query(format!("ping: {}", driver_error(&e))))?
                .into_first_result()
                .await
                .map_err(|e| CaboodleError::Query(format!("ping results: {}", driver_error(&e))))?;
            Ok(())
        })
        .await
        .map_err(|_| CaboodleError::Query("ping timed out after 15 seconds".into()))?
    }
}
