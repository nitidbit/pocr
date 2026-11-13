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

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;

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
    fn repository_storage_failure_maps_to_internal_with_cause() {
        let err = ApiError::from(RepositoryError::storage(std::io::Error::other("boom")));
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(std::error::Error::source(&err).is_some());
    }

    #[tokio::test]
    async fn internal_response_hides_the_cause() {
        let err = ApiError::internal(std::io::Error::other("secret-connection-string"));

        let response = err.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"], "internal_error");
        assert!(!body.to_string().contains("secret-connection-string"));
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
            ApiError::internal(std::io::Error::other("x")),
        ];
        for err in variants {
            assert!(!err.code().is_empty());
            assert_ne!(err.status(), StatusCode::default());
        }
    }
}
