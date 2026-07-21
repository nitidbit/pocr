//! User identity endpoint for the frontend.

use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::extractors::AuthUser;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeResponse {
    email: String,
    name: String,
    given_name: String,
    picture: String,
    groups: Vec<String>,
}

/// `GET /api/me` — Return the authenticated user's identity from Vouch headers.
///
/// The user identity comes from the `X-Vouch-User` header via [`AuthUser`],
/// which rejects unauthenticated requests in release builds and falls back to a
/// development identity otherwise. The remaining profile fields are read from
/// the companion `X-Vouch-*` headers.
pub async fn get_me(
    user: AuthUser,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    let header_field = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned()
    };

    let groups = headers
        .get("X-Vouch-Groups")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').map(|g| g.trim().to_owned()).collect())
        .unwrap_or_default();

    Ok(Json(MeResponse {
        email: user.0,
        name: header_field("X-Vouch-Name"),
        given_name: header_field("X-Vouch-Given-Name"),
        picture: header_field("X-Vouch-Picture"),
        groups,
    }))
}
