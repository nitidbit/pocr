//! `ApiError` response mapping: HTTP status, stable error code, and body shape.

use api_http::ApiError;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use pocr_core::ports::RepositoryError;
use pocr_core::study::StudyError;
use serde_json::Value;

async fn render(error: ApiError) -> (StatusCode, Value) {
    let response = error.into_response();
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body is readable")
        .to_bytes();
    let body = serde_json::from_slice(&bytes).expect("body is JSON");
    (status, body)
}

#[tokio::test]
async fn each_variant_maps_to_its_status_and_code() {
    let cases = [
        (ApiError::NotFound, StatusCode::NOT_FOUND, "not_found"),
        (
            ApiError::PreconditionFailed("x".into()),
            StatusCode::PRECONDITION_FAILED,
            "precondition_failed",
        ),
        (
            ApiError::BadRequest("x".into()),
            StatusCode::BAD_REQUEST,
            "bad_request",
        ),
        (
            ApiError::Forbidden("x".into()),
            StatusCode::FORBIDDEN,
            "forbidden",
        ),
        (
            ApiError::Conflict("x".into()),
            StatusCode::CONFLICT,
            "conflict",
        ),
        (
            ApiError::ValidationFailed(vec!["x".into()]),
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
        ),
        (
            ApiError::internal(std::io::Error::other("x")),
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
        ),
    ];

    for (error, expected_status, expected_code) in cases {
        let (status, body) = render(error).await;

        assert_eq!(status, expected_status);
        assert_eq!(body["error"], expected_code);
    }
}

#[tokio::test]
async fn validation_failure_lists_each_message_under_details() {
    let error = ApiError::ValidationFailed(vec!["first".into(), "second".into()]);

    let (_, body) = render(error).await;

    assert_eq!(body["details"], serde_json::json!(["first", "second"]));
}

#[tokio::test]
async fn internal_error_hides_the_cause_from_the_client() {
    let error = ApiError::internal(std::io::Error::other("secret-connection-string"));

    let (_, body) = render(error).await;

    assert!(!body.to_string().contains("secret-connection-string"));
}

#[tokio::test]
async fn repository_not_found_maps_to_not_found() {
    let (status, body) = render(RepositoryError::NotFound.into()).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn repository_conflict_maps_to_bad_request() {
    let (status, _) = render(RepositoryError::Conflict("dup".into()).into()).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn repository_storage_failure_maps_to_internal_error() {
    let source = RepositoryError::storage(std::io::Error::other("boom"));

    let (status, body) = render(source.into()).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "internal_error");
}

#[tokio::test]
async fn frozen_study_config_maps_to_conflict() {
    let (status, body) = render(StudyError::ConfigFrozen.into()).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "conflict");
}

#[tokio::test]
async fn invalid_study_configuration_maps_to_validation_failure() {
    let error = StudyError::InvalidConfiguration("bad".into());

    let (status, body) = render(error.into()).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["details"], serde_json::json!(["bad"]));
}
