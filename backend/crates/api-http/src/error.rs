//! API error types and HTTP response mapping.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use pocr_core::ports::RepositoryError;
use pocr_core::study::StudyError;

/// Errors returned by API handlers, mapped to HTTP status codes.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,
    #[error("precondition failed: {0}")]
    PreconditionFailed(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("validation failed")]
    ValidationFailed(Vec<String>),
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<RepositoryError> for ApiError {
    fn from(e: RepositoryError) -> Self {
        match e {
            RepositoryError::NotFound => ApiError::NotFound,
            RepositoryError::Conflict(msg) => ApiError::BadRequest(msg),
            RepositoryError::Connection(msg) => ApiError::Internal(msg),
            RepositoryError::Internal(msg) => ApiError::Internal(msg),
        }
    }
}

impl From<StudyError> for ApiError {
    fn from(e: StudyError) -> Self {
        match e {
            // An illegal transition or an edit to a frozen config is a state
            // conflict, not a malformed request.
            StudyError::InvalidTransition { .. } | StudyError::ConfigFrozen => {
                ApiError::Conflict(e.to_string())
            }
            StudyError::InvalidConfiguration(msg) => ApiError::ValidationFailed(vec![msg]),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match &self {
            ApiError::ValidationFailed(details) => {
                let body = json!({
                    "error": "validation_failed",
                    "details": details,
                });
                (StatusCode::UNPROCESSABLE_ENTITY, Json(body)).into_response()
            }
            _ => {
                let (status, message) = match &self {
                    ApiError::NotFound => (StatusCode::NOT_FOUND, "not found"),
                    ApiError::PreconditionFailed(_) => {
                        (StatusCode::PRECONDITION_FAILED, "precondition failed")
                    }
                    ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad request"),
                    ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
                    ApiError::Internal(_) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
                    }
                    ApiError::ValidationFailed(_) => unreachable!(),
                };

                let body = json!({
                    "error": message,
                    "detail": self.to_string(),
                });

                (status, Json(body)).into_response()
            }
        }
    }
}
