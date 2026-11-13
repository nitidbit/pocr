//! CDS Hooks request wire shapes.

use pocr_core::cds_hooks::{CdsHookRequest, FhirAuthorization};

/// An Epic-shaped `patient-view` request with a full, `snake_case`
/// OAuth 2.0 token under `fhirAuthorization`.
fn epic_request_json() -> serde_json::Value {
    serde_json::json!({
        "hookInstance": "d1577c69-dfbe-44ad-ba6d-3e05e953b2ea",
        "fhirServer": "https://tst.epic.example/api/FHIR/R4",
        "hook": "patient-view",
        "fhirAuthorization": {
            "access_token": "eyJraWQiOiJlLXAtb2F1dGgtY2VydA",
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": "patient/*.read",
            "subject": "cds-service4"
        },
        "context": {
            "patientId": "eXbCe123",
            "encounterId": "eKq456",
            "userId": "Practitioner/ePr789"
        }
    })
}

#[test]
fn deserializes_snake_case_fhir_authorization() {
    let req: CdsHookRequest = serde_json::from_value(epic_request_json()).expect("valid request");

    let auth = req.fhir_authorization.expect("fhirAuthorization present");

    assert_eq!(
        auth.access_token.as_deref(),
        Some("eyJraWQiOiJlLXAtb2F1dGgtY2VydA")
    );
    assert_eq!(auth.token_type.as_deref(), Some("Bearer"));
    assert_eq!(auth.expires_in, Some(3600));
    assert_eq!(auth.scope.as_deref(), Some("patient/*.read"));
    assert_eq!(auth.subject.as_deref(), Some("cds-service4"));
}

#[test]
fn deserializes_fhir_authorization_missing_access_token() {
    let mut json = epic_request_json();
    json["fhirAuthorization"]
        .as_object_mut()
        .expect("authorization is an object")
        .remove("access_token");

    let req: CdsHookRequest = serde_json::from_value(json).expect("valid request");

    let auth = req.fhir_authorization.expect("fhirAuthorization present");
    assert_eq!(auth.access_token, None);
    assert_eq!(auth.token_type.as_deref(), Some("Bearer"));
}

#[test]
fn deserializes_request_without_fhir_authorization() {
    let mut json = epic_request_json();
    json.as_object_mut()
        .expect("request is an object")
        .remove("fhirAuthorization");

    let req: CdsHookRequest = serde_json::from_value(json).expect("valid request");

    assert!(req.fhir_authorization.is_none());
}

#[test]
fn serializes_authorization_fields_as_snake_case() {
    let req: CdsHookRequest = serde_json::from_value(epic_request_json()).expect("valid request");

    let value = serde_json::to_value(&req).expect("serializable");

    let auth = &value["fhirAuthorization"];
    assert_eq!(auth["access_token"], "eyJraWQiOiJlLXAtb2F1dGgtY2VydA");
    assert_eq!(auth["token_type"], "Bearer");
    assert_eq!(auth["expires_in"], 3600);
}

#[test]
fn skips_none_authorization_fields_on_serialize() {
    let auth = FhirAuthorization {
        access_token: Some("tok".into()),
        token_type: None,
        expires_in: None,
        scope: None,
        subject: None,
    };

    let value = serde_json::to_value(&auth).expect("serializable");

    let obj = value.as_object().expect("authorization is an object");
    assert_eq!(obj.len(), 1);
    assert_eq!(obj["access_token"], "tok");
}
