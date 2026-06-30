//! Study CRUD HTTP handlers.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use tracing::info;
use uuid::Uuid;

use pocr_core::random::{Arm, Stratum};
use pocr_core::study::{RandomizationConfig, RandomizationStrategy, Study, StudyStatus};

use crate::error::ApiError;
use crate::extractors::AuthUser;
use crate::AppState;

/// Request body for creating a study.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStudyRequest {
    pub name: String,
    pub description: String,
    pub strategy: RandomizationStrategy,
    pub arms: Vec<CreateArmRequest>,
    #[serde(default)]
    pub strata: Vec<CreateStratumRequest>,
    pub block_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateArmRequest {
    pub name: String,
    pub ratio: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStratumRequest {
    pub name: String,
    pub context: String,
    pub path: String,
}

/// Request body for updating a study.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStudyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<StudyStatus>,
}

/// List all studies for the current user.
pub async fn list_studies(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<impl IntoResponse, ApiError> {
    let studies = state.study_repo.list_for_user(user.id()).await?;
    Ok(Json(studies))
}

/// Get a single study by ID.
pub async fn get_study(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let study = state
        .study_repo
        .find_by_id(&id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(study))
}

/// Create a new study.
pub async fn create_study(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateStudyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let arms: Vec<Arm> = req
        .arms
        .into_iter()
        .map(|a| Arm {
            id: Uuid::now_v7(),
            name: a.name,
            ratio: a.ratio,
            locator: None,
        })
        .collect();

    let strata: Vec<Stratum> = req
        .strata
        .into_iter()
        .map(|s| Stratum {
            id: Uuid::now_v7().to_string(),
            name: s.name,
            context: s.context,
            path: s.path,
        })
        .collect();

    let config = RandomizationConfig {
        strategy: req.strategy,
        arms,
        keys: vec![],
        strata,
        block_size: req.block_size,
    };

    let study = Study::new(req.name, req.description, config);
    let saved = state.study_repo.save(&study).await?;

    state
        .user_study_repo
        .add_user(user.id(), &saved.id, "owner")
        .await?;

    info!(
        event_type = "study.created",
        study_id = %saved.id,
        user_id = user.id(),
        study_name = %saved.name,
        "study created"
    );

    Ok((StatusCode::CREATED, Json(saved)))
}

/// Update an existing study.
pub async fn update_study(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateStudyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut study = state
        .study_repo
        .find_by_id(&id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if let Some(name) = req.name {
        study.name = name;
    }
    if let Some(description) = req.description {
        study.description = description;
    }
    if let Some(status) = req.status {
        study.status = status;
    }
    study.updated_at = chrono::Utc::now();

    let saved = state.study_repo.save(&study).await?;

    info!(
        event_type = "study.updated",
        study_id = %saved.id,
        user_id = user.id(),
        "study updated"
    );

    Ok(Json(saved))
}

/// Delete a study.
pub async fn delete_study(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    state.study_repo.delete(&id).await?;

    info!(
        event_type = "study.deleted",
        study_id = %id,
        user_id = user.id(),
        "study deleted"
    );

    Ok(StatusCode::NO_CONTENT)
}

/// Request body for setting an arm's locator.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLocatorRequest {
    pub locator: u64,
}

/// Set the Epic locator ID for a specific arm in a study.
pub async fn set_arm_locator(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((id, arm_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<SetLocatorRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut study = state
        .study_repo
        .find_by_id(&id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let arm = study
        .randomization_config
        .arms
        .iter_mut()
        .find(|a| a.id == arm_id)
        .ok_or(ApiError::NotFound)?;

    arm.locator = Some(req.locator);
    study.updated_at = chrono::Utc::now();

    let saved = state.study_repo.save(&study).await?;

    // Persist locator → study mapping for CDS Hooks lookups
    state.locator_repo.save_mapping(req.locator, &id).await?;

    info!(
        event_type = "study.locator_set",
        study_id = %id,
        arm_id = %arm_id,
        locator = req.locator,
        user_id = user.id(),
        "arm locator set"
    );

    Ok(Json(saved))
}
