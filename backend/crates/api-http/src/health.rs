//! Health check endpoints.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct ReadyResponse {
    status: &'static str,
    postgres: &'static str,
    redis: &'static str,
}

/// Liveness probe for orchestrator health checks.
pub async fn live() -> impl IntoResponse {
    Json(HealthResponse { status: "alive" })
}

/// Readiness probe — verifies PG and Redis connectivity.
pub async fn ready(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let pg_ok = sqlx::query("SELECT 1")
        .execute(&state.pg_pool)
        .await
        .is_ok();

    let redis_ok = state.redis_pool.ping().await.is_ok();

    let pg_status = if pg_ok { "ok" } else { "error" };
    let redis_status = if redis_ok { "ok" } else { "error" };

    let overall = if pg_ok && redis_ok {
        "ready"
    } else {
        "degraded"
    };

    let status_code = if pg_ok && redis_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadyResponse {
            status: overall,
            postgres: pg_status,
            redis: redis_status,
        }),
    )
}
