//! Randomization history HTTP handlers.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use pocr_core::environment::Environment;
use pocr_core::random::{Arm, Randomization, Strata, Unit};

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

/// camelCase HTTP view of a [`Randomization`].
///
/// The core [`Randomization`] serializes its fields as snake_case
/// (`study_id`, `issued_at`), but the frontend consumes camelCase. Rather than
/// change the shared core wire format — which the PostgreSQL repository and
/// gRPC layer also round-trip — this DTO is local to the HTTP layer and only
/// remaps the top-level field names. `unit`, `strata`, and `arm` delegate to
/// their existing serde impls, which already emit the shapes the frontend
/// expects.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RandomizationResponse {
    pub id: Uuid,
    pub study_id: Uuid,
    pub environment: Environment,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strata: Option<Strata>,
    pub unit: Unit,
    pub arm: Arm,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_patient_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_csn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_user_id: Option<String>,
}

impl From<Randomization> for RandomizationResponse {
    fn from(r: Randomization) -> Self {
        Self {
            id: r.id,
            study_id: r.study_id,
            environment: r.environment,
            strata: r.strata,
            unit: r.unit,
            arm: r.arm,
            issued_at: r.issued_at,
            epic_patient_id: r.epic_patient_id,
            epic_csn: r.epic_csn,
            epic_user_id: r.epic_user_id,
        }
    }
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
    let response: Vec<RandomizationResponse> =
        randomizations.into_iter().map(Into::into).collect();
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocr_core::random::{Arm, SimpleUnit, Unit};

    fn make_randomization() -> Randomization {
        let arm = Arm::new("Treatment A", 1);
        let unit = Unit::Simple(SimpleUnit::PatientId("P-001".to_string()));
        Randomization::simple(&Uuid::now_v7(), Environment::Production, &unit, &arm)
    }

    #[test]
    fn serializes_top_level_fields_as_camel_case() {
        let dto = RandomizationResponse::from(make_randomization());
        let value = serde_json::to_value(&dto).unwrap();
        let obj = value.as_object().unwrap();

        assert!(obj.contains_key("studyId"), "expected studyId key");
        assert!(obj.contains_key("issuedAt"), "expected issuedAt key");
        assert!(!obj.contains_key("study_id"), "snake_case must not leak");
        assert!(!obj.contains_key("issued_at"), "snake_case must not leak");
    }

    #[test]
    fn includes_arm_name() {
        let dto = RandomizationResponse::from(make_randomization());
        let value = serde_json::to_value(&dto).unwrap();
        assert_eq!(value["arm"]["name"], "Treatment A");
    }

    #[test]
    fn issued_at_is_rfc3339_parseable_date() {
        let dto = RandomizationResponse::from(make_randomization());
        let value = serde_json::to_value(&dto).unwrap();
        let issued_at = value["issuedAt"].as_str().unwrap();
        // A valid RFC3339 timestamp is what `new Date(...)` needs to avoid
        // "Invalid Date" on the frontend.
        assert!(chrono::DateTime::parse_from_rfc3339(issued_at).is_ok());
    }

    #[test]
    fn preserves_unit_kind_data_shape() {
        let dto = RandomizationResponse::from(make_randomization());
        let value = serde_json::to_value(&dto).unwrap();
        assert!(value["unit"].get("kind").is_some());
        assert!(value["unit"].get("data").is_some());
    }
}
