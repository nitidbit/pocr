//! API error types and HTTP response mapping.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tracing::error;

use pocr_core::ports::RepositoryError;
use pocr_core::study::StudyError;

/// Errors returned by API handlers, mapped to HTTP status codes.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The resource does not exist, or the caller may not know it exists (404).
    #[error("not found")]
    NotFound,
    /// A precondition for the request is unmet (412).
    #[error("precondition failed: {0}")]
    PreconditionFailed(String),
    /// The request is malformed or semantically invalid (400).
    #[error("bad request: {0}")]
    BadRequest(String),
    /// The caller is known but lacks the required role (403).
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// The request conflicts with the resource's current state (409).
    #[error("conflict: {0}")]
    Conflict(String),
    /// The request body failed validation, with one message per failure (422).
    #[error("validation failed")]
    ValidationFailed(Vec<String>),
    /// An unexpected failure (500).
    ///
    /// The cause is logged server-side and never sent to clients.
    #[error("internal error")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl ApiError {
    /// Wraps an unexpected failure as a 500, keeping `source` for the server-side log.
    pub fn internal(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(source))
    }

    /// The HTTP status this error maps to.
    fn status(&self) -> StatusCode {
        match self {
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::PreconditionFailed(_) => StatusCode::PRECONDITION_FAILED,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Forbidden(_) => StatusCode::FORBIDDEN,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::ValidationFailed(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The stable machine-readable error code returned in the response body.
    fn code(&self) -> &'static str {
        match self {
            ApiError::NotFound => "not_found",
            ApiError::PreconditionFailed(_) => "precondition_failed",
            ApiError::BadRequest(_) => "bad_request",
            ApiError::Forbidden(_) => "forbidden",
            ApiError::Conflict(_) => "conflict",
            ApiError::ValidationFailed(_) => "validation_failed",
            ApiError::Internal(_) => "internal_error",
        }
    }
}

impl From<RepositoryError> for ApiError {
    fn from(e: RepositoryError) -> Self {
        match e {
            RepositoryError::NotFound => ApiError::NotFound,
            RepositoryError::Conflict(msg) => ApiError::BadRequest(msg),
            _ => ApiError::internal(e),
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
            _ => ApiError::internal(e),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.code();
        let status = self.status();
        let body = match &self {
            ApiError::ValidationFailed(details) => json!({
                "error": code,
                "details": details,
            }),
            _ => json!({
                "error": code,
                "detail": self.to_string(),
            }),
        };

        if matches!(self, ApiError::Internal(_)) {
            // anyhow renders the whole source chain; the JSON log formatter
            // records only an error's own Display.
            error!(error = %format!("{:#}", anyhow::Error::from(self)), "request failed");
        }

        (status, Json(body)).into_response()
    }
}
