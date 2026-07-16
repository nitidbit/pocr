//! Axum extractors for common request context.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;

/// Authenticated user ID extracted from the `X-Vouch-User` header.
///
/// In production (release builds), Vouch proxy must set this header after
/// successful OAuth/OIDC auth — requests without it are rejected with 401.
/// In debug builds, falls back to `"system"` for local development.
pub struct AuthUser(pub String);

impl AuthUser {
    pub fn id(&self) -> &str {
        &self.0
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let header_value = parts
            .headers
            .get("X-Vouch-User")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());

        match header_value {
            Some(user_id) => Ok(AuthUser(user_id)),
            None => {
                #[cfg(debug_assertions)]
                {
                    tracing::warn!("X-Vouch-User header missing, falling back to \"system\" (dev mode)");
                    Ok(AuthUser("system".to_owned()))
                }
                #[cfg(not(debug_assertions))]
                {
                    Err((StatusCode::UNAUTHORIZED, "missing authentication header"))
                }
            }
        }
    }
}
