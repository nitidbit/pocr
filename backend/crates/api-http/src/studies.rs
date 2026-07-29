//! Study CRUD HTTP handlers.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, warn};
use uuid::Uuid;

use pocr_core::audit::{event_type, AuditEvent};
use pocr_core::ports::AuditRepository;
use pocr_core::random::{Arm, KeyVariant, Stratum};
use pocr_core::study::{RandomizationConfig, RandomizationStrategy, Study, StudyStatus};

use crate::error::ApiError;
use crate::extractors::AuthUser;
use crate::AppState;

/// Append an audit event for a study mutation, best-effort.
///
/// A failed audit write is logged at WARN and swallowed so a transient audit
/// store outage never fails an otherwise-successful mutation. `event_type`
/// should be one of the [`event_type`] constants; `detail` carries any
/// event-specific fields (pass [`Value::Null`] when there are none).
pub(crate) async fn record_audit(
    audit_repo: &dyn AuditRepository,
    study_id: &Uuid,
    event_type: &str,
    actor: &str,
    detail: Value,
) {
    let event = AuditEvent::record(study_id, event_type, actor, detail);
    if let Err(error) = audit_repo.append(&event).await {
        warn!(
            study_id = %study_id,
            event_type,
            error = %error,
            "audit event append failed"
        );
    }
}

/// Request body for creating a study.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStudyRequest {
    pub name: String,
    pub description: String,
    pub strategy: RandomizationStrategy,
    pub arms: Vec<CreateArmRequest>,
    #[serde(default)]
    pub keys: Vec<KeyVariant>,
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

/// Request body for updating a study's cosmetic fields.
///
/// Status changes are handled separately by [`transition_study`] so the
/// lifecycle state machine is always enforced.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStudyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Request body for a lifecycle transition.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionStudyRequest {
    pub status: StudyStatus,
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

/// List a study's audit history, newest-first.
pub async fn list_audit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let events = state.audit_repo.list_by_study(&id).await?;
    Ok(Json(events))
}

/// Validate a create study request, returning a list of errors.
fn validate_create_request(req: &CreateStudyRequest) -> Vec<String> {
    let mut errors = Vec::new();

    if req.name.trim().is_empty() {
        errors.push("name is required".to_string());
    } else if req.name.len() > 200 {
        errors.push("name must be 200 characters or fewer".to_string());
    }

    if req.arms.len() < 2 {
        errors.push("at least 2 arms are required".to_string());
    }

    for (i, arm) in req.arms.iter().enumerate() {
        if arm.name.trim().is_empty() {
            errors.push(format!("arm[{i}].name is required"));
        }
        if arm.ratio == 0 {
            errors.push(format!("arm[{i}].ratio must be positive"));
        }
    }

    if let Some(block_size) = req.block_size {
        let ratio_sum: u32 = req.arms.iter().map(|a| a.ratio).sum();
        if ratio_sum > 0 && block_size % ratio_sum != 0 {
            errors.push(format!(
                "block_size ({block_size}) must be a multiple of the sum of arm ratios ({ratio_sum})"
            ));
        }
    }

    errors
}

/// Validate a batch of arm-ratio updates against a study's current config.
///
/// Returns the collected human-readable errors; an empty vec means the update
/// is valid. The updates are applied to a copy of the current ratios so that
/// cross-arm invariants are checked on the resulting state. An update is
/// rejected when it is empty, references an unknown arm, names the same arm
/// twice, would leave every arm with a ratio of zero (no arm could be
/// allocated), or would make a fixed block size no longer a multiple of the
/// ratio sum.
fn validate_ratio_updates(config: &RandomizationConfig, updates: &[ArmRatioUpdate]) -> Vec<String> {
    let mut errors = Vec::new();

    if updates.is_empty() {
        errors.push("at least one arm ratio is required".to_string());
        return errors;
    }

    let mut ratios: std::collections::HashMap<Uuid, u32> =
        config.arms.iter().map(|a| (a.id, a.ratio)).collect();

    let mut seen = std::collections::HashSet::new();
    for update in updates {
        if !seen.insert(update.arm_id) {
            errors.push(format!("duplicate arm {}", update.arm_id));
            continue;
        }
        match ratios.get_mut(&update.arm_id) {
            Some(ratio) => *ratio = update.ratio,
            None => errors.push(format!("unknown arm {}", update.arm_id)),
        }
    }

    if !errors.is_empty() {
        return errors;
    }

    let ratio_sum: u32 = ratios.values().sum();
    if ratio_sum == 0 {
        errors.push("at least one arm must have a positive ratio".to_string());
        return errors;
    }

    if let Some(block_size) = config.block_size
        && block_size % ratio_sum != 0
    {
        errors.push(format!(
            "block_size ({block_size}) must be a multiple of the sum of arm ratios ({ratio_sum})"
        ));
    }

    errors
}

/// Create a new study.
pub async fn create_study(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateStudyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let errors = validate_create_request(&req);
    if !errors.is_empty() {
        return Err(ApiError::ValidationFailed(errors));
    }

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
        keys: req.keys,
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
    if let Some(ref name) = req.name {
        let mut errors = Vec::new();
        if name.trim().is_empty() {
            errors.push("name is required".to_string());
        } else if name.len() > 200 {
            errors.push("name must be 200 characters or fewer".to_string());
        }
        if !errors.is_empty() {
            return Err(ApiError::ValidationFailed(errors));
        }
    }

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
    study.updated_at = chrono::Utc::now();

    let saved = state.study_repo.save(&study).await?;
    let _ = state.study_cache.invalidate_study(&saved.id).await;

    info!(
        event_type = "study.updated",
        study_id = %saved.id,
        user_id = user.id(),
        "study updated"
    );

    Ok(Json(saved))
}

/// Transition a study to a new lifecycle status.
///
/// Enforces the study state machine (`Study::transition_to`): illegal
/// transitions and go-live with an invalid config are rejected. Going live
/// (`Draft → Active`) freezes the allocation configuration.
pub async fn transition_study(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<TransitionStudyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut study = state
        .study_repo
        .find_by_id(&id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let from = study.status;
    study.transition_to(req.status)?;

    let saved = state.study_repo.save(&study).await?;
    let _ = state.study_cache.invalidate_study(&saved.id).await;

    info!(
        event_type = "study.transitioned",
        study_id = %saved.id,
        user_id = user.id(),
        from = from.as_str(),
        to = saved.status.as_str(),
        "study transitioned"
    );

    record_audit(
        state.audit_repo.as_ref(),
        &saved.id,
        event_type::STUDY_TRANSITIONED,
        user.id(),
        serde_json::json!({ "from": from.as_str(), "to": saved.status.as_str() }),
    )
    .await;

    Ok(Json(saved))
}

/// Delete all non-production randomizations and cached blocks for a study.
///
/// Resets test data so a fresh non-production run starts from a clean
/// sequence. Production state is never touched.
pub async fn purge_nonproduction(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // Confirm the study exists before purging.
    state
        .study_repo
        .find_by_id(&id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let records = state.randomization_repo.purge_nonproduction(&id).await?;
    let blocks = state.block_cache.purge_nonproduction(&id).await?;

    info!(
        event_type = "study.nonproduction_purged",
        study_id = %id,
        user_id = user.id(),
        records_removed = records,
        blocks_removed = blocks,
        "non-production data purged"
    );

    record_audit(
        state.audit_repo.as_ref(),
        &id,
        event_type::NONPRODUCTION_PURGED,
        user.id(),
        serde_json::json!({ "recordsRemoved": records, "blocksRemoved": blocks }),
    )
    .await;

    Ok(Json(serde_json::json!({
        "recordsRemoved": records,
        "blocksRemoved": blocks,
    })))
}

/// Delete a study.
pub async fn delete_study(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    state.study_repo.delete(&id).await?;
    let _ = state.study_cache.invalidate_study(&id).await;

    info!(
        event_type = "study.deleted",
        study_id = %id,
        user_id = user.id(),
        "study deleted"
    );

    // audit_events has no FK to studies, so this deletion record survives the
    // delete — the retained "who/when" trail a regulated study requires.
    record_audit(
        state.audit_repo.as_ref(),
        &id,
        event_type::STUDY_DELETED,
        user.id(),
        Value::Null,
    )
    .await;

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

    record_audit(
        state.audit_repo.as_ref(),
        &id,
        event_type::LOCATOR_SET,
        user.id(),
        serde_json::json!({ "armId": arm_id, "locator": req.locator }),
    )
    .await;

    Ok(Json(saved))
}

/// A single arm's new allocation ratio.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmRatioUpdate {
    pub arm_id: Uuid,
    pub ratio: u32,
}

/// Request body for updating arm allocation ratios.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRatiosRequest {
    pub arms: Vec<ArmRatioUpdate>,
}

/// Update the allocation ratios of one or more arms in a study.
///
/// Ratios are editable in both draft and active studies. The change takes
/// effect only for permuted blocks generated *after* it is saved: block
/// contents are frozen in the cache at generation time
/// ([`pocr_core::random::RandomPermutedBlock`]), so any partially-consumed
/// cached block keeps its original ratio until exhausted, at which point the
/// next block is generated honoring the new ratio. Existing cached blocks are
/// deliberately neither regenerated nor invalidated.
///
/// Unlike other study mutations, the audit write here is **not** best-effort:
/// the change is a regulated allocation edit that must be auditable, so the
/// [`event_type::RATIO_CHANGED`] event (carrying the per-arm old→new ratios) is
/// appended *before* the study is saved. If the append fails the mutation is
/// abandoned and the caller receives an error — a ratio change can never
/// persist without its audit record.
///
/// # Errors
///
/// Return [`ApiError::NotFound`] if the study does not exist.
/// Return [`ApiError::ValidationFailed`] if the updates are invalid
/// (see [`validate_ratio_updates`]).
/// Return [`ApiError::Internal`] if the audit event or study cannot be saved.
pub async fn update_ratios(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRatiosRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut study = state
        .study_repo
        .find_by_id(&id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let errors = validate_ratio_updates(&study.randomization_config, &req.arms);
    if !errors.is_empty() {
        return Err(ApiError::ValidationFailed(errors));
    }

    // Compute the effective changes without mutating, so an all-unchanged
    // request is a no-op (no save, no audit event) and the audit detail is
    // ready before anything persists.
    let current: std::collections::HashMap<Uuid, u32> = study
        .randomization_config
        .arms
        .iter()
        .map(|a| (a.id, a.ratio))
        .collect();
    let changes: Vec<Value> = req
        .arms
        .iter()
        .filter(|update| current.get(&update.arm_id) != Some(&update.ratio))
        .map(|update| {
            serde_json::json!({
                "armId": update.arm_id,
                "from": current[&update.arm_id],
                "to": update.ratio,
            })
        })
        .collect();

    if changes.is_empty() {
        return Ok(Json(study));
    }

    // Audit-first: append the record before persisting the change. A failed
    // append aborts the mutation so the study is never saved without its trail.
    let event = AuditEvent::record(
        &id,
        event_type::RATIO_CHANGED,
        user.id(),
        serde_json::json!({ "changes": changes }),
    );
    state.audit_repo.append(&event).await?;

    for update in &req.arms {
        if let Some(arm) = study
            .randomization_config
            .arms
            .iter_mut()
            .find(|a| a.id == update.arm_id)
        {
            arm.ratio = update.ratio;
        }
    }
    study.updated_at = chrono::Utc::now();

    let saved = state.study_repo.save(&study).await?;
    let _ = state.study_cache.invalidate_study(&saved.id).await;

    info!(
        event_type = "study.ratio_changed",
        study_id = %saved.id,
        user_id = user.id(),
        arms_changed = changes.len(),
        "arm ratios updated"
    );

    Ok(Json(saved))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use pocr_core::ports::RepositoryError;

    #[derive(Default)]
    struct RecordingAuditRepo {
        appended: Mutex<Vec<AuditEvent>>,
        fail: bool,
    }

    #[async_trait]
    impl AuditRepository for RecordingAuditRepo {
        async fn append(&self, event: &AuditEvent) -> Result<(), RepositoryError> {
            if self.fail {
                return Err(RepositoryError::Internal("boom".into()));
            }
            self.appended.lock().unwrap().push(event.clone());
            Ok(())
        }

        async fn list_by_study(
            &self,
            _study_id: &Uuid,
        ) -> Result<Vec<AuditEvent>, RepositoryError> {
            Ok(self.appended.lock().unwrap().clone())
        }
    }

    #[tokio::test]
    async fn record_audit_appends_event_with_supplied_fields() {
        let repo = RecordingAuditRepo::default();
        let study_id = Uuid::now_v7();

        record_audit(
            &repo,
            &study_id,
            event_type::STUDY_TRANSITIONED,
            "alice@ucsf.edu",
            serde_json::json!({ "to": "active" }),
        )
        .await;

        let appended = repo.appended.lock().unwrap();
        assert_eq!(appended.len(), 1);
        assert_eq!(appended[0].study_id, study_id);
        assert_eq!(appended[0].event_type, event_type::STUDY_TRANSITIONED);
        assert_eq!(appended[0].actor, "alice@ucsf.edu");
    }

    #[tokio::test]
    async fn record_audit_swallows_append_failure() {
        let repo = RecordingAuditRepo {
            fail: true,
            ..Default::default()
        };

        record_audit(
            &repo,
            &Uuid::now_v7(),
            event_type::STUDY_DELETED,
            "bob@ucsf.edu",
            Value::Null,
        )
        .await;
    }

    fn arm(ratio: u32) -> Arm {
        Arm {
            id: Uuid::now_v7(),
            name: String::new(),
            ratio,
            locator: None,
        }
    }

    fn config(arms: Vec<Arm>, block_size: Option<u32>) -> RandomizationConfig {
        RandomizationConfig {
            strategy: RandomizationStrategy::Block,
            arms,
            keys: Vec::new(),
            strata: Vec::new(),
            block_size,
        }
    }

    #[test]
    fn validate_ratio_updates_accepts_positive_ratios() {
        let config = config(vec![arm(1), arm(1)], None);
        let updates = vec![
            ArmRatioUpdate { arm_id: config.arms[0].id, ratio: 3 },
            ArmRatioUpdate { arm_id: config.arms[1].id, ratio: 1 },
        ];

        assert!(validate_ratio_updates(&config, &updates).is_empty());
    }

    #[test]
    fn validate_ratio_updates_allows_a_single_zero_ratio_when_another_is_positive() {
        let config = config(vec![arm(1), arm(1)], None);
        let updates = vec![ArmRatioUpdate { arm_id: config.arms[0].id, ratio: 0 }];

        assert!(validate_ratio_updates(&config, &updates).is_empty());
    }

    #[test]
    fn validate_ratio_updates_rejects_empty_update() {
        let config = config(vec![arm(1), arm(1)], None);

        let errors = validate_ratio_updates(&config, &[]);

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("at least one arm ratio"));
    }

    #[test]
    fn validate_ratio_updates_rejects_unknown_arm() {
        let config = config(vec![arm(1), arm(1)], None);
        let updates = vec![ArmRatioUpdate { arm_id: Uuid::now_v7(), ratio: 2 }];

        let errors = validate_ratio_updates(&config, &updates);

        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("unknown arm"));
    }

    #[test]
    fn validate_ratio_updates_rejects_duplicate_arm() {
        let config = config(vec![arm(1), arm(1)], None);
        let updates = vec![
            ArmRatioUpdate { arm_id: config.arms[0].id, ratio: 2 },
            ArmRatioUpdate { arm_id: config.arms[0].id, ratio: 3 },
        ];

        let errors = validate_ratio_updates(&config, &updates);

        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("duplicate arm"));
    }

    #[test]
    fn validate_ratio_updates_rejects_all_arms_zero() {
        let config = config(vec![arm(1), arm(1)], None);
        let updates = vec![
            ArmRatioUpdate { arm_id: config.arms[0].id, ratio: 0 },
            ArmRatioUpdate { arm_id: config.arms[1].id, ratio: 0 },
        ];

        let errors = validate_ratio_updates(&config, &updates);

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("positive ratio"));
    }

    #[test]
    fn validate_ratio_updates_rejects_ratio_sum_incompatible_with_block_size() {
        let config = config(vec![arm(1), arm(1)], Some(6));
        let updates = vec![ArmRatioUpdate { arm_id: config.arms[0].id, ratio: 3 }];

        let errors = validate_ratio_updates(&config, &updates);

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("block_size"));
    }

    #[test]
    fn validate_ratio_updates_accepts_ratio_sum_that_divides_block_size() {
        let config = config(vec![arm(1), arm(1)], Some(6));
        let updates = vec![ArmRatioUpdate { arm_id: config.arms[0].id, ratio: 2 }];

        assert!(validate_ratio_updates(&config, &updates).is_empty());
    }
}
