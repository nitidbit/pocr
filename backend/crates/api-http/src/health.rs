use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
struct HealthResponse {
    status: &'static str,
}

/// Liveness probe for orchestrator health checks.
pub async fn live() -> impl IntoResponse {
    Json(HealthResponse { status: "alive" })
}

/// Readiness probe for orchestrator traffic routing.
pub async fn ready() -> impl IntoResponse {
    Json(HealthResponse { status: "ready" })
}
