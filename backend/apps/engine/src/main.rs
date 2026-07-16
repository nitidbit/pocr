use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::serve;
use pocr_core::environment::{Environment, EnvironmentMap};
use pocr_engine::error::{EngineError, EngineResult};
use pocr_engine::health::RedisHealthCheck;
use pocr_engine::service::Engine;
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> EngineResult<()> {
    let _guard = observability::init(observability::Config {
        default_filter: "info,sqlx=warn,hyper=warn,tower_http=trace",
        ..observability::Config::for_service("pocr-engine")
    });

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = env::var("API_PORT").unwrap_or_else(|_| "3000".into());
    let _grpc_port = env::var("GRPC_PORT").unwrap_or_else(|_| "50051".into());
    let database_url = match env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            #[cfg(debug_assertions)]
            {
                tracing::warn!("DATABASE_URL not set, using local dev default");
                "postgres://pocr:pocr@localhost:5432/pocr".into()
            }
            #[cfg(not(debug_assertions))]
            {
                panic!("DATABASE_URL must be set in production");
            }
        }
    };
    let redis_url =
        env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());

    let addr: SocketAddr = format!("{host}:{port}").parse().map_err(|e| {
        EngineError::Unknown(format!("invalid address {host}:{port}: {e}"))
    })?;

    // Initialize PostgreSQL
    let pg_pool = infra_pg::create_pool(&database_url).await.map_err(|e| {
        EngineError::Unknown(format!("failed to connect to database: {e}"))
    })?;

    infra_pg::run_migrations(&pg_pool).await.map_err(|e| {
        EngineError::Unknown(format!("failed to run migrations: {e}"))
    })?;

    info!(event_type = "database.ready", "database ready");

    // Initialize Redis
    let redis_pool = Arc::new(infra_redis::RedisPool::new(&redis_url).map_err(|e| {
        EngineError::Unknown(format!("failed to connect to redis: {e}"))
    })?);

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
    let randomization_repo =
        Arc::new(infra_pg::PgRandomizationRepository::new(pg_pool.clone()));
    let user_study_repo = Arc::new(infra_pg::PgUserStudyRepository::new(pg_pool.clone()));
    let locator_repo = Arc::new(infra_pg::PgLocatorRepository::new(pg_pool.clone()));
    let block_cache = Arc::new(infra_redis::RedisBlockCache::new(redis_pool.clone()));
    let study_cache = Arc::new(infra_redis::RedisStudyCache::new(redis_pool.clone()));
    let redis_health = Arc::new(RedisHealthCheck::new(redis_pool));

    // Build randomization service
    let randomization_service = Arc::new(Engine::new(
        randomization_repo.clone(),
        block_cache.clone(),
    ));

    // Parse the fhirServer URL → environment map (fails closed on unknown URLs).
    let environment_map = parse_environment_map(env::var("FHIR_ENV_MAP").ok().as_deref());
    if environment_map.is_empty() {
        tracing::warn!(
            "FHIR_ENV_MAP is empty — all CDS Hooks randomization requests will be rejected"
        );
    }

    // TLS certificate path (optional — mounted in production)
    let cert_path = env::var("CERT_PATH").ok();

    // Build app state
    let state = Arc::new(api_http::AppState {
        study_repo,
        randomization_repo,
        block_cache,
        user_study_repo,
        locator_repo,
        randomization_service,
        study_cache,
        environment_map,
        pg_pool,
        redis_pool: redis_health,
        jwt_secret,
        cert_path,
        started_at: std::time::Instant::now(),
    });

    let app = api_http::app(state);
    let listener = TcpListener::bind(addr).await.map_err(|e| {
        EngineError::Unknown(format!("failed to bind {addr}: {e}"))
    })?;

    info!(event_type = "server.started", %addr, "server listening");

    serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| EngineError::Unknown(e.to_string()))?;

    info!("server shut down");
    Ok(())
}

/// Parse the `FHIR_ENV_MAP` variable into an [`EnvironmentMap`].
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

/// Wait for SIGTERM or ctrl_c, whichever arrives first.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
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
