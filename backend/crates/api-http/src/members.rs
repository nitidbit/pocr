//! Study membership HTTP handlers.
//!
//! Manages who has access to a study and in what role. Membership management
//! is owner-only; any member may view the roster. This is distinct from the
//! cross-instance JWT federation flow in [`crate::sharing`].

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use tracing::info;
use uuid::Uuid;

use pocr_core::ports::StudyMember;

use crate::AppState;
use crate::error::ApiError;
use crate::extractors::{AuthUser, normalize_email};

/// A study access role, ordered so that `Owner >= Member`.
///
/// Declaration order defines the ordering: [`Role::Member`] is the lower
/// privilege, [`Role::Owner`] the higher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Member,
    Owner,
}

impl Role {
    /// The wire/database string form of the role.
    fn as_str(self) -> &'static str {
        match self {
            Role::Member => "member",
            Role::Owner => "owner",
        }
    }
}

/// Parse a role string, returning `None` for any unrecognized value.
fn parse_role(raw: &str) -> Option<Role> {
    match raw {
        "member" => Some(Role::Member),
        "owner" => Some(Role::Owner),
        _ => None,
    }
}

/// Validate a normalized email address. Requires a non-empty value containing
/// an `@`.
fn validate_email(email: &str) -> Result<(), ApiError> {
    if email.is_empty() {
        return Err(ApiError::BadRequest("email is required".to_string()));
    }
    if !email.contains('@') {
        return Err(ApiError::BadRequest("email must contain '@'".to_string()));
    }
    Ok(())
}

/// Decide whether a caller holding `current` role may perform an operation
/// requiring at least `min_role`.
///
/// A caller with no membership yields [`ApiError::NotFound`] — this hides the
/// existence of studies they cannot see. A member attempting an owner-only
/// operation yields [`ApiError::Forbidden`].
fn decide_access(current: Option<Role>, min_role: Role) -> Result<(), ApiError> {
    match current {
        None => Err(ApiError::NotFound),
        Some(role) if role >= min_role => Ok(()),
        Some(_) => Err(ApiError::Forbidden(
            "only owners can manage access".to_string(),
        )),
    }
}

/// Determine whether removing or demoting `target_email` would leave the study
/// with no owner.
///
/// Returns `true` only when `target_email` is currently the sole owner. Email
/// comparison assumes both sides are already normalized.
fn would_orphan(members: &[StudyMember], target_email: &str) -> bool {
    let owners: Vec<&StudyMember> = members.iter().filter(|m| m.role == "owner").collect();
    owners.len() == 1 && owners[0].user_id == target_email
}

/// Fetch the caller's role and authorize against `min_role`.
async fn require_role(
    state: &AppState,
    study_id: &Uuid,
    user: &AuthUser,
    min_role: Role,
) -> Result<(), ApiError> {
    let current = state
        .user_study_repo
        .get_role(user.id(), study_id)
        .await?
        .as_deref()
        .and_then(parse_role);
    decide_access(current, min_role)
}

/// Request body for adding a member or changing their role.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddMemberRequest {
    pub email: String,
    pub role: String,
}

/// Request body for updating an existing member's role.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRoleRequest {
    pub role: String,
}

/// Look up a single member of a study by normalized email after a write.
async fn find_member(
    state: &AppState,
    study_id: &Uuid,
    email: &str,
) -> Result<StudyMember, ApiError> {
    state
        .user_study_repo
        .list_members(study_id)
        .await?
        .into_iter()
        .find(|m| m.user_id == email)
        .ok_or(ApiError::NotFound)
}

/// `GET /studies/{id}/members` — list every member of a study.
///
/// Any member may view the roster.
pub async fn list(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&state, &id, &user, Role::Member).await?;
    let members = state.user_study_repo.list_members(&id).await?;
    Ok(Json(members))
}

/// `POST /studies/{id}/members` — add a member or upsert their role.
///
/// Owner-only. Adding an email that already has access updates the role.
/// Demoting the study's last owner to `member` is rejected with `409`.
/// Returns `201` with the resulting member.
pub async fn add(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&state, &id, &user, Role::Owner).await?;

    let email = normalize_email(&req.email);
    validate_email(&email)?;
    let role = parse_role(&req.role)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid role: {}", req.role)))?;

    if role == Role::Member {
        let members = state.user_study_repo.list_members(&id).await?;
        if would_orphan(&members, &email) {
            return Err(ApiError::Conflict(
                "study must retain at least one owner".to_string(),
            ));
        }
    }

    state
        .user_study_repo
        .add_user(&email, &id, role.as_str())
        .await?;

    info!(
        event_type = "study.member_added",
        study_id = %id,
        user_id = user.id(),
        role = role.as_str(),
        "study member added"
    );

    let member = find_member(&state, &id, &email).await?;
    Ok((StatusCode::CREATED, Json(member)))
}

/// `PUT /studies/{id}/members/{email}` — change an existing member's role.
///
/// Owner-only. Demoting the study's last owner to `member` is rejected with
/// `409`. Returns `200` with the updated member.
pub async fn update_role(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((id, email)): Path<(Uuid, String)>,
    Json(req): Json<UpdateRoleRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&state, &id, &user, Role::Owner).await?;

    let email = normalize_email(&email);
    validate_email(&email)?;
    let role = parse_role(&req.role)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid role: {}", req.role)))?;

    if role == Role::Member {
        let members = state.user_study_repo.list_members(&id).await?;
        if would_orphan(&members, &email) {
            return Err(ApiError::Conflict(
                "study must retain at least one owner".to_string(),
            ));
        }
    }

    state
        .user_study_repo
        .add_user(&email, &id, role.as_str())
        .await?;

    info!(
        event_type = "study.member_role_changed",
        study_id = %id,
        user_id = user.id(),
        role = role.as_str(),
        "study member role changed"
    );

    let member = find_member(&state, &id, &email).await?;
    Ok((StatusCode::OK, Json(member)))
}

/// `DELETE /studies/{id}/members/{email}` — revoke a member's access.
///
/// Owner-only. Removing the study's last owner is rejected with `409`.
/// Returns `204`.
pub async fn remove(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((id, email)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&state, &id, &user, Role::Owner).await?;

    let email = normalize_email(&email);

    let members = state.user_study_repo.list_members(&id).await?;
    if would_orphan(&members, &email) {
        return Err(ApiError::Conflict(
            "study must retain at least one owner".to_string(),
        ));
    }

    state.user_study_repo.remove_user(&email, &id).await?;

    info!(
        event_type = "study.member_removed",
        study_id = %id,
        user_id = user.id(),
        "study member removed"
    );

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn member(email: &str, role: &str) -> StudyMember {
        StudyMember {
            user_id: email.to_string(),
            role: role.to_string(),
            joined_at: Utc::now(),
        }
    }

    #[test]
    fn parse_role_accepts_known_roles() {
        assert_eq!(parse_role("member"), Some(Role::Member));
        assert_eq!(parse_role("owner"), Some(Role::Owner));
    }

    #[test]
    fn parse_role_rejects_unknown_role() {
        assert_eq!(parse_role("admin"), None);
        assert_eq!(parse_role(""), None);
        assert_eq!(parse_role("Owner"), None);
    }

    #[test]
    fn owner_outranks_member() {
        assert!(Role::Owner >= Role::Member);
        assert!(Role::Owner >= Role::Owner);
        assert!(Role::Member >= Role::Member);
        assert!(!(Role::Member >= Role::Owner));
    }

    #[test]
    fn normalize_email_trims_and_lowercases() {
        assert_eq!(normalize_email("  Alice@Example.COM "), "alice@example.com");
    }

    #[test]
    fn validate_email_rejects_empty() {
        assert!(matches!(validate_email(""), Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn validate_email_rejects_missing_at_sign() {
        assert!(matches!(
            validate_email("alice"),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn validate_email_accepts_well_formed_address() {
        assert!(validate_email("alice@example.com").is_ok());
    }

    #[test]
    fn decide_access_rejects_non_member_as_not_found() {
        assert!(matches!(
            decide_access(None, Role::Member),
            Err(ApiError::NotFound)
        ));
    }

    #[test]
    fn decide_access_rejects_member_on_owner_op_as_forbidden() {
        assert!(matches!(
            decide_access(Some(Role::Member), Role::Owner),
            Err(ApiError::Forbidden(_))
        ));
    }

    #[test]
    fn decide_access_allows_member_on_member_op() {
        assert!(decide_access(Some(Role::Member), Role::Member).is_ok());
    }

    #[test]
    fn decide_access_allows_owner_on_owner_op() {
        assert!(decide_access(Some(Role::Owner), Role::Owner).is_ok());
    }

    #[test]
    fn would_orphan_true_when_target_is_sole_owner() {
        let members = [member("owner@x.com", "owner"), member("m@x.com", "member")];
        assert!(would_orphan(&members, "owner@x.com"));
    }

    #[test]
    fn would_orphan_false_when_another_owner_remains() {
        let members = [
            member("owner1@x.com", "owner"),
            member("owner2@x.com", "owner"),
        ];
        assert!(!would_orphan(&members, "owner1@x.com"));
    }

    #[test]
    fn would_orphan_false_when_target_is_not_an_owner() {
        let members = [member("owner@x.com", "owner"), member("m@x.com", "member")];
        assert!(!would_orphan(&members, "m@x.com"));
    }
}
