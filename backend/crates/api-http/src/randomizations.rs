//! Randomization history HTTP handlers.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use pocr_core::environment::Environment;

use crate::error::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
    /// Which environment's records to list. Defaults to production.
    #[serde(default)]
    pub environment: Option<Environment>,
}

fn default_limit() -> u32 {
    50
}

/// List randomizations for a study within one environment, with pagination.
///
/// Defaults to production records; pass `?environment=nonproduction` to view
/// test data.
pub async fn list_randomizations(
    State(state): State<Arc<AppState>>,
    Path(study_id): Path<Uuid>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, ApiError> {
    let environment = params.environment.unwrap_or(Environment::Production);
    let randomizations = state
        .randomization_repo
        .find_by_study(&study_id, environment, params.limit, params.offset)
        .await?;
    Ok(Json(randomizations))
}
