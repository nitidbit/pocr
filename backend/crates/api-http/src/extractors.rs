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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    async fn extract(header: Option<&str>) -> Result<AuthUser, (StatusCode, &'static str)> {
        let mut builder = Request::builder();
        if let Some(value) = header {
            builder = builder.header("X-Vouch-User", value);
        }
        let request = builder.body(()).unwrap();
        let (mut parts, ()) = request.into_parts();
        AuthUser::from_request_parts(&mut parts, &()).await
    }

    #[tokio::test]
    async fn extracts_user_id_from_header() {
        let user = extract(Some("alice@example.com")).await.unwrap();
        assert_eq!(user.id(), "alice@example.com");
    }

    #[cfg(debug_assertions)]
    #[tokio::test]
    async fn falls_back_to_system_when_header_missing_in_debug() {
        let user = extract(None).await.unwrap();
        assert_eq!(user.id(), "system");
    }

    #[cfg(not(debug_assertions))]
    #[tokio::test]
    async fn rejects_missing_header_in_release() {
        let result = extract(None).await;
        assert_eq!(result.unwrap_err().0, StatusCode::UNAUTHORIZED);
    }
}
