use secrecy::ExposeSecret;
use tiberius::{AuthMethod, Client, Config as TiberiusConfig};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;
use tracing::{info, instrument};

use crate::config::CaboodleConfig;
use crate::error::{CaboodleError, CaboodleResult};

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
        tib_config.trust_cert();

        let tcp = TcpStream::connect(tib_config.get_addr())
            .await
            .map_err(|e| CaboodleError::Connection(format!("TCP connect failed: {e}")))?;

        tcp.set_nodelay(true)
            .map_err(|e| CaboodleError::Connection(format!("set_nodelay failed: {e}")))?;

        let client = Client::connect(tib_config, tcp.compat_write())
            .await
            .map_err(|e| CaboodleError::Connection(format!("TDS connect failed: {e}")))?;

        info!("connected to Caboodle");
        Ok(Self { client })
    }
}
