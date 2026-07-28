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
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("validation failed")]
    ValidationFailed(Vec<String>),
    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
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
            RepositoryError::Connection(msg) => ApiError::Internal(msg),
            RepositoryError::Internal(msg) => ApiError::Internal(msg),
            _ => ApiError::Internal(e.to_string()),
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
            _ => ApiError::Internal(e.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.code();
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

        (self.status(), Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_conflict_maps_to_bad_request() {
        let err = ApiError::from(RepositoryError::Conflict("dup".into()));
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn repository_not_found_maps_to_not_found() {
        let err = ApiError::from(RepositoryError::NotFound);
        assert_eq!(err.status(), StatusCode::NOT_FOUND);
        assert_eq!(err.code(), "not_found");
    }

    #[test]
    fn study_frozen_config_maps_to_conflict() {
        let err = ApiError::from(StudyError::ConfigFrozen);
        assert_eq!(err.status(), StatusCode::CONFLICT);
        assert_eq!(err.code(), "conflict");
    }

    #[test]
    fn study_invalid_configuration_maps_to_validation_failed() {
        let err = ApiError::from(StudyError::InvalidConfiguration("bad".into()));
        assert_eq!(err.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.code(), "validation_failed");
    }

    #[test]
    fn forbidden_maps_to_forbidden_status_and_code() {
        let err = ApiError::Forbidden("owner only".into());
        assert_eq!(err.status(), StatusCode::FORBIDDEN);
        assert_eq!(err.code(), "forbidden");
    }

    #[test]
    fn every_variant_has_a_status_and_code() {
        let variants = [
            ApiError::NotFound,
            ApiError::PreconditionFailed("x".into()),
            ApiError::BadRequest("x".into()),
            ApiError::Forbidden("x".into()),
            ApiError::Conflict("x".into()),
            ApiError::ValidationFailed(vec!["x".into()]),
            ApiError::Internal("x".into()),
        ];
        for err in variants {
            assert!(!err.code().is_empty());
            assert_ne!(err.status(), StatusCode::default());
        }
    }
}
