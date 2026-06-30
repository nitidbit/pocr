//! JWT-based study sharing — token generation and join flow.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extractors::AuthUser;
use crate::AppState;

/// JWT claims for a study sharing token.
#[derive(Debug, Serialize, Deserialize)]
struct ShareClaims {
    /// Subject: the study ID being shared.
    sub: String,
    /// Issued at (Unix timestamp).
    iat: u64,
    /// Expiration (Unix timestamp).
    exp: u64,
    /// Role granted to the joiner.
    role: String,
}

/// Response when generating a share token.
#[derive(Debug, Serialize)]
struct ShareResponse {
    token: String,
}

/// Request body when joining via a share token.
#[derive(Debug, Deserialize)]
pub struct JoinRequest {
    pub token: String,
}

/// Response when successfully joining a study.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JoinResponse {
    study_id: Uuid,
    role: String,
}

/// Generate a share token for a study. Token is valid for 7 days.
pub async fn share_study(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // Verify study exists
    state
        .study_repo
        .find_by_id(&id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let now = chrono::Utc::now().timestamp() as u64;
    let claims = ShareClaims {
        sub: id.to_string(),
        iat: now,
        exp: now + 7 * 24 * 60 * 60, // 7 days
        role: "member".to_string(),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(format!("token generation failed: {e}")))?;

    info!(
        event_type = "study.share_token_generated",
        study_id = %id,
        user_id = user.id(),
        "share token generated"
    );

    Ok((StatusCode::CREATED, Json(ShareResponse { token })))
}

/// Join a study using a share token.
pub async fn join_study(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<JoinRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut validation = Validation::default();
    validation.validate_exp = true;

    let token_data = decode::<ShareClaims>(
        &req.token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &validation,
    )
    .map_err(|e| ApiError::BadRequest(format!("invalid or expired token: {e}")))?;

    let study_id: Uuid = token_data
        .claims
        .sub
        .parse()
        .map_err(|_| ApiError::BadRequest("invalid study ID in token".to_string()))?;

    // Verify study still exists
    state
        .study_repo
        .find_by_id(&study_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Add user to study
    state
        .user_study_repo
        .add_user(user.id(), &study_id, &token_data.claims.role)
        .await?;

    info!(
        event_type = "study.user_joined",
        study_id = %study_id,
        user_id = user.id(),
        role = %token_data.claims.role,
        "user joined study"
    );

    Ok((
        StatusCode::OK,
        Json(JoinResponse {
            study_id,
            role: token_data.claims.role,
        }),
    ))
}
