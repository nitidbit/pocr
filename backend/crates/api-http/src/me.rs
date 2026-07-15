//! User identity endpoint for the frontend.

use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

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
pub async fn get_me(headers: HeaderMap) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    let headers = &headers;

    let email = headers
        .get("X-Vouch-User")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    let Some(email) = email else {
        #[cfg(debug_assertions)]
        {
            return Ok(Json(MeResponse {
                email: "andrew.robinson@ucsf.edu".to_owned(),
                name: "Andrew Robinson".to_owned(),
                given_name: "Andrew".to_owned(),
                picture: String::new(),
                groups: vec!["ctsi-randomization-admins".to_owned()],
            }));
        }
        #[cfg(not(debug_assertions))]
        {
            return Err((StatusCode::UNAUTHORIZED, "missing authentication header"));
        }
    };

    let name = headers
        .get("X-Vouch-Name")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();

    let given_name = headers
        .get("X-Vouch-Given-Name")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();

    let picture = headers
        .get("X-Vouch-Picture")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();

    let groups = headers
        .get("X-Vouch-Groups")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').map(|g| g.trim().to_owned()).collect())
        .unwrap_or_default();

    Ok(Json(MeResponse {
        email,
        name,
        given_name,
        picture,
        groups,
    }))
}
