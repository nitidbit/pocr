//! Standalone connectivity check against a real Caboodle instance.
//!
//! Verifies reachability, SQL Server service-account authentication, TLS,
//! and a trivial query without running the sync pipeline.
//!
//! ```bash
//! cd backend
//! CABOODLE_HOST=apexcaboodleprd.ucsfmedicalcenter.org CABOODLE_DATABASE=CDW \
//!   CABOODLE_USERNAME='svc_pocr' CABOODLE_PASSWORD='...' \
//!   cargo run -p infra-caboodle --example connect_smoke_test
//! ```

use infra_caboodle::{CaboodleClient, CaboodleConfig, CaboodleError};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), CaboodleError> {
    let _guard = observability::init(observability::Config::for_service("caboodle-smoke-test"));
    let config = CaboodleConfig::from_env().inspect_err(|e| {
        error!(error = %e, "invalid Caboodle configuration");
    })?;
    let client = CaboodleClient::connect(&config).await?;
    info!("connected, pinging");
    client.ping().await?;
    info!("Caboodle connectivity check passed");
    Ok(())
}
