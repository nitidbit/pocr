//! The `pocr-engine` binary: configures dependencies from the environment and
//! serves the HTTP API.

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use axum::serve;
use pocr_core::environment::{Environment, EnvironmentMap};
use pocr_engine::Engine;
use pocr_engine::{EngineError, EngineResult};
use pocr_engine::{PgHealthCheck, RedisHealthCheck};
use tokio::net::TcpListener;
use tracing::{error, info};

#[tokio::main]
async fn main() -> ExitCode {
    let _guard = observability::init(observability::Config {
        default_filter: "info,sqlx=warn,hyper=warn,tower_http=trace",
        ..observability::Config::for_service("pocr-engine")
    });

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!(error = %format!("{:#}", anyhow::Error::from(e)), "engine failed");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> EngineResult<()> {
    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = env::var("API_PORT").unwrap_or_else(|_| "3000".into());
    let database_url = resolve_secret("DATABASE_URL", "postgres://pocr:pocr@localhost:5432/pocr")?;
    let redis_url = resolve_secret("REDIS_URL", "redis://localhost:6379")?;

    let address = format!("{host}:{port}");
    let addr: SocketAddr = address
        .parse()
        .map_err(|source| EngineError::InvalidAddress { address, source })?;

    // Initialize PostgreSQL
    let pg_pool = infra_pg::create_pool(&database_url)
        .await
        .map_err(EngineError::Database)?;

    infra_pg::run_migrations(&pg_pool)
        .await
        .map_err(EngineError::Migrations)?;

    info!(event_type = "database.ready", "database ready");

    // Initialize Redis
    let redis_pool = Arc::new(infra_redis::RedisPool::new(&redis_url).map_err(EngineError::Redis)?);

    // JWT secret for study sharing
    let jwt_secret = match env::var("JWT_SECRET") {
        Ok(secret) => secret,
        Err(_) => {
            #[cfg(debug_assertions)]
            {
                tracing::warn!("JWT_SECRET not set, using insecure dev default");
                "pocr-dev-secret-change-in-production".into()
            }
            #[cfg(not(debug_assertions))]
            {
                panic!("JWT_SECRET must be set in production");
            }
        }
    };

    // Build repositories
    let study_repo = Arc::new(infra_pg::PgStudyRepository::new(pg_pool.clone()));
    let randomization_repo = Arc::new(infra_pg::PgRandomizationRepository::new(pg_pool.clone()));
    let user_study_repo = Arc::new(infra_pg::PgUserStudyRepository::new(pg_pool.clone()));
    let audit_repo = Arc::new(infra_pg::PgAuditRepository::new(pg_pool.clone()));
    let locator_repo = Arc::new(infra_pg::PgLocatorRepository::new(pg_pool.clone()));
    let block_cache = Arc::new(infra_redis::RedisBlockCache::new(Arc::clone(&redis_pool)));
    let study_cache = Arc::new(infra_redis::RedisStudyCache::new(Arc::clone(&redis_pool)));
    let pg_health = Arc::new(PgHealthCheck::new(pg_pool.clone()));
    let redis_health = Arc::new(RedisHealthCheck::new(redis_pool));

    // Build randomization service
    let randomization_service = Arc::new(Engine::new(
        Arc::clone(&randomization_repo) as _,
        Arc::clone(&block_cache) as _,
    ));

    // Parse the fhirServer URL → environment map (fails closed on unknown URLs).
    let environment_map = parse_environment_map(env::var("FHIR_ENV_MAP").ok().as_deref());
    if environment_map.is_empty() {
        tracing::warn!(
            "FHIR_ENV_MAP is empty — all CDS Hooks randomization requests will be rejected"
        );
    }

    // TLS certificate path (optional — mounted in production)
    let cert_path = env::var_os("CERT_PATH").map(|path| PathBuf::from(path).into());

    // Build app state
    let datasets = api_http::DatasetsState {
        catalog: Arc::new(
            infra_datasets::from_env()
                .map_err(EngineError::Catalog)?
                .unwrap_or_default(),
        ),
        datasets: Arc::new(infra_pg::PgDatasetRepository::new(pg_pool.clone())),
        members: Arc::clone(&user_study_repo) as _,
        studies: Arc::clone(&study_repo) as _,
        audit: Arc::clone(&audit_repo) as _,
    };
    let state = api_http::AppState {
        study_repo,
        randomization_repo,
        block_cache,
        user_study_repo,
        audit_repo,
        locator_repo,
        randomization_service,
        study_cache,
        environment_map: Arc::new(environment_map),
        pg_health,
        redis_health,
        jwt_secret: jwt_secret.into(),
        cert_path,
        datasets,
        started_at: std::time::Instant::now(),
    };

    let app = api_http::app(state);
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| EngineError::Bind { addr, source })?;

    info!(event_type = "server.started", %addr, "server listening");

    serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(EngineError::Serve)?;

    info!("server shut down");
    Ok(())
}

/// Resolves a connection URL that may be supplied directly or via a secret file.
///
/// Resolution order for a variable named `var`:
/// 1. `<var>_FILE` — read the file, trim whitespace, and use its contents (the
///    full URL including any credentials). Docker/Kubernetes mount secrets here.
/// 2. `<var>` — use the environment value directly.
/// 3. Fall back to `dev_default` in debug builds; panic in release builds so a
///    misconfigured production deployment fails fast rather than using a default.
///
/// # Errors
///
/// Returns [`EngineError::SecretFile`] if `<var>_FILE` is set but cannot be read.
fn resolve_secret(var: &str, dev_default: &str) -> EngineResult<String> {
    if let Ok(path) = env::var(format!("{var}_FILE")) {
        let contents =
            std::fs::read_to_string(&path).map_err(|source| EngineError::SecretFile {
                variable: var.to_owned(),
                path: path.into(),
                source,
            })?;
        return Ok(contents.trim().to_owned());
    }

    if let Ok(value) = env::var(var) {
        return Ok(value);
    }

    #[cfg(debug_assertions)]
    {
        tracing::warn!(var, "secret not set, using local dev default");
        Ok(dev_default.to_owned())
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = dev_default;
        panic!("{var} (or {var}_FILE) must be set in production");
    }
}

/// Parses the `FHIR_ENV_MAP` variable into an [`EnvironmentMap`].
///
/// Format: semicolon-separated `prefix=environment` pairs, where environment
/// is `production` or `nonproduction`. Example:
/// `https://prd.epic.example/=production;https://tst.epic.example/=nonproduction`.
/// Malformed or unrecognized-environment entries are skipped with a warning.
fn parse_environment_map(raw: Option<&str>) -> EnvironmentMap {
    let Some(raw) = raw else {
        return EnvironmentMap::default();
    };

    let entries = raw.split(';').filter_map(|pair| {
        let pair = pair.trim();
        if pair.is_empty() {
            return None;
        }
        let (prefix, env) = pair.split_once('=')?;
        let (prefix, env) = (prefix.trim(), env.trim());
        match Environment::parse(env) {
            Some(environment) if !prefix.is_empty() => Some((prefix.to_string(), environment)),
            _ => {
                tracing::warn!(entry = pair, "ignoring malformed FHIR_ENV_MAP entry");
                None
            }
        }
    });

    EnvironmentMap::new(entries)
}

/// Waits for SIGTERM or `ctrl_c`, whichever arrives first.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to register SIGTERM handler");

    #[cfg(unix)]
    tokio::select! {
        _ = ctrl_c => {},
        _ = sigterm.recv() => {},
    }

    #[cfg(not(unix))]
    ctrl_c.await.expect("failed to listen for ctrl+c");

    info!(event_type = "server.shutdown", "shutdown signal received");
}
