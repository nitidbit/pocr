//! Axum extractors for common request context.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;

/// Normalize an email-form identity: trim surrounding whitespace and lowercase.
///
/// Google addresses are case-insensitive; applying this at every boundary that
/// ingests an email keeps a single person from acquiring divergent identities
/// (e.g. an owner row stored lowercase but a role lookup made against the raw
/// header value).
pub(crate) fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Authenticated user ID extracted from the `X-Vouch-User` header.
///
/// In production (release builds), Vouch proxy must set this header after
/// successful OAuth/OIDC auth — requests without it are rejected with 401.
/// In debug builds, falls back to `"system"` for local development.
///
/// The identity is normalized ([`normalize_email`]) so downstream comparisons
/// against stored membership rows use one canonical form.
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
            Some(user_id) => Ok(AuthUser(normalize_email(&user_id))),
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

    #[tokio::test]
    async fn normalizes_mixed_case_header_to_lowercase() {
        let user = extract(Some(" Bob@X.com ")).await.unwrap();
        assert_eq!(user.id(), "bob@x.com");
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
