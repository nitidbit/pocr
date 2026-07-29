//! CDS Hooks HTTP handlers — the integration point with Epic EHR.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use tracing::{info, warn};

use pocr_core::cds_hooks::{
    Card, CdsHookRequest, CdsHookResponse, CdsServiceDescriptor, CdsServiceDiscovery, Indicator,
    Source,
};

use crate::error::ApiError;
use crate::AppState;

/// `GET /cds-services` — CDS Hooks service discovery.
pub async fn discover_services() -> impl IntoResponse {
    let discovery = CdsServiceDiscovery {
        services: vec![CdsServiceDescriptor {
            hook: "patient-view".to_string(),
            title: "Randomization Service".to_string(),
            description: "Service to provide randomization assignments for clinical trials"
                .to_string(),
            id: "randomization".to_string(),
            prefetch: Some(json!({
                "patient": "Patient/{{context.patientId}}",
                "encounter": "Encounter/{{context.encounterId}}",
                "user": "{{context.userId}}"
            })),
        }],
    };
    Json(discovery)
}

/// `POST /cds-services/randomization` — Execute randomization via CDS Hooks.
pub async fn randomize(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CdsHookRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Resolve the deployment environment from the fhirServer URL. Fails closed:
    // an unknown or missing instance is refused before any state is touched, so
    // a test instance can never randomize into production data.
    let environment = match state
        .environment_map
        .resolve(request.fhir_server.as_deref())
    {
        Ok(env) => env,
        Err(e) => {
            warn!(error = %e, "rejecting randomization from unrecognized fhirServer");
            return Ok((
                StatusCode::OK,
                Json(CdsHookResponse {
                    cards: vec![info_card(
                        "Unrecognized environment",
                        "This EHR instance is not configured for randomization.",
                    )],
                }),
            ));
        }
    };

    // Extract locator from Epic extension
    let locator = extract_locator(&request);

    let Some(locator) = locator else {
        warn!("CDS Hooks request missing locator");
        return Ok((
            StatusCode::OK,
            Json(CdsHookResponse {
                cards: vec![info_card(
                    "No locator found",
                    "Request did not contain a valid study locator.",
                )],
            }),
        ));
    };

    // Look up study by locator (cache-first, fallback to PG)
    let study = match state.study_cache.get_study_id_for_locator(locator).await {
        Ok(Some(study_id)) => {
            // Cache hit on locator → try to get full study from cache
            match state.study_cache.get_cached_study(&study_id).await {
                Ok(Some(study)) => Some(study),
                _ => {
                    // Cache miss on study body — fallback to PG
                    let study = state.study_repo.find_by_id(&study_id).await?;
                    if let Some(ref s) = study {
                        let _ = state.study_cache.cache_study(s).await;
                    }
                    study
                }
            }
        }
        _ => {
            // Cache miss on locator — fallback to PG
            let study = state.study_repo.find_by_locator(locator).await?;
            if let Some(ref s) = study {
                let _ = state.study_cache.cache_locator_mapping(locator, &s.id).await;
                let _ = state.study_cache.cache_study(s).await;
            }
            study
        }
    };

    let Some(study) = study else {
        warn!(locator, "No study found for locator");
        return Err(ApiError::PreconditionFailed(
            "No study configured for this locator".to_string(),
        ));
    };

    // Build randomization request from CDS context
    let randomization_request = pocr_core::random::RandomizationRequest {
        locator: Some(locator),
        patient_id: request.context.patient_id.clone(),
        encounter_id: request.context.encounter_id.clone(),
        user_id: request.context.user_id.clone(),
        context: request.prefetch.clone(),
    };

    // Execute randomization via the service
    let result = state
        .randomization_service
        .randomize(&study, environment, &randomization_request)
        .await;

    match result {
        // Lifecycle gate rejections are expected, not errors: surface them as
        // an informational card rather than a 500.
        Err(
            e @ (pocr_core::error::RandomizationError::StudyNotActive
            | pocr_core::error::RandomizationError::StudyClosed),
        ) => {
            info!(study_id = %study.id, %environment, reason = %e, "randomization gated");
            Ok((
                StatusCode::OK,
                Json(CdsHookResponse {
                    cards: vec![info_card("Randomization unavailable", &e.to_string())],
                }),
            ))
        }
        Ok(randomization) => {
            // Locator gate: the patient is only "randomized" for this request
            // when the assigned arm maps to the locator that triggered it.
            // An arm with no locator, or one bound to a different locator,
            // yields no card — the clinician sees nothing, matching the POC.
            let Some(arm_locator) = randomization.arm.locator else {
                info!(
                    study_id = %study.id,
                    arm_id = %randomization.arm.id,
                    "Assigned arm has no locator — no card"
                );
                return Err(ApiError::PreconditionFailed(
                    "Assigned arm has no locator".to_string(),
                ));
            };

            if arm_locator != locator {
                info!(
                    study_id = %study.id,
                    arm_id = %randomization.arm.id,
                    arm_locator,
                    request_locator = locator,
                    "Assigned arm is for a different locator — no card"
                );
                return Err(ApiError::PreconditionFailed(
                    "Assigned arm is for a different locator".to_string(),
                ));
            }

            info!(
                study_id = %study.id,
                arm_id = %randomization.arm.id,
                "Randomization issued"
            );

            let card = Card {
                uuid: Some(randomization.id.to_string()),
                summary: format!("Randomized: {}", randomization.arm.name),
                detail: Some(format!(
                    "Study: {}\nArm: {} (ratio {})\nAssignment ID: {}",
                    study.name, randomization.arm.name, randomization.arm.ratio, randomization.id
                )),
                indicator: Indicator::Warning,
                source: Source {
                    label: "POCR".to_string(),
                    url: Some("https://randomization.ucsf.edu".to_string()),
                    icon: None,
                    topic: None,
                },
                suggestions: None,
                links: None,
            };

            Ok((StatusCode::OK, Json(CdsHookResponse { cards: vec![card] })))
        }
        Err(e) => {
            warn!(error = %e, study_id = %study.id, "Randomization failed");
            Err(ApiError::Internal(e.to_string()))
        }
    }
}

/// Extract the Epic locator ID from the CDS Hooks request extension.
fn extract_locator(request: &CdsHookRequest) -> Option<u64> {
    // Epic sends locator in extension as criteria-id
    request
        .extension
        .as_ref()?
        .get("com.epic.cdshooks.request.criteria-id")?
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| {
            // Fallback: try numeric value directly
            request
                .extension
                .as_ref()?
                .get("com.epic.cdshooks.request.criteria-id")?
                .as_u64()
        })
}

fn info_card(summary: &str, detail: &str) -> Card {
    Card {
        uuid: None,
        summary: summary.to_string(),
        detail: Some(detail.to_string()),
        indicator: Indicator::Info,
        source: Source {
            label: "POCR".to_string(),
            url: None,
            icon: None,
            topic: None,
        },
        suggestions: None,
        links: None,
    }
}
