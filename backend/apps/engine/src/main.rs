use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::serve;
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
    let redis_health = Arc::new(RedisHealthCheck::new(redis_pool));

    // Build randomization service
    let randomization_service = Arc::new(Engine::new(
        randomization_repo.clone(),
        block_cache,
    ));

    // Build app state
    let state = Arc::new(api_http::AppState {
        study_repo,
        randomization_repo,
        user_study_repo,
        locator_repo,
        randomization_service,
        pg_pool,
        redis_pool: redis_health,
        jwt_secret,
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
