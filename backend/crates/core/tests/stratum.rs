//! Resolving stratification values from request context.

use pocr_core::error::RandomizationError;
use pocr_core::random::{RandomizationRequest, SimpleStratum, Strata, Stratum};
use serde_json::json;

fn request_with_context() -> RandomizationRequest {
    RandomizationRequest {
        locator: None,
        patient_id: None,
        encounter_id: None,
        user_id: None,
        fhir_server: None,
        context: Some(json!({
            "Patient": {
                "gender": "male",
                "age": "45"
            }
        })),
    }
}

fn stratum(id: &str, name: &str, path: &str) -> Stratum {
    Stratum {
        id: id.to_owned(),
        name: name.to_owned(),
        context: "Patient".to_owned(),
        path: path.to_owned(),
    }
}

fn age_stratum() -> Stratum {
    stratum("age", "Age", "/age")
}

#[test]
fn simple_stratum_extracts_value_from_context() {
    let result =
        SimpleStratum::try_parse(&age_stratum(), &request_with_context()).expect("path resolves");

    assert!(matches!(
        result,
        SimpleStratum::FhirPath { ref name, ref value } if name == "age" && value == "45"
    ));
}

#[test]
fn simple_stratum_rejects_missing_context_as_not_found() {
    let request = RandomizationRequest {
        context: None,
        ..request_with_context()
    };

    let result = SimpleStratum::try_parse(&age_stratum(), &request);

    assert!(matches!(result, Err(RandomizationError::NotFound)));
}

#[test]
fn simple_stratum_rejects_unresolved_path_as_not_found() {
    let missing = stratum("x", "X", "/nonexistent");

    let result = SimpleStratum::try_parse(&missing, &request_with_context());

    assert!(matches!(result, Err(RandomizationError::NotFound)));
}

#[test]
fn strata_reject_empty_slice_as_not_found() {
    let result = Strata::try_parse(&[], &request_with_context());

    assert!(matches!(result, Err(RandomizationError::NotFound)));
}

#[test]
fn strata_are_simple_for_single_stratum() {
    let result =
        Strata::try_parse(&[age_stratum()], &request_with_context()).expect("path resolves");

    assert!(matches!(result, Strata::Simple(_)));
}

#[test]
fn strata_are_composite_for_multiple_strata() {
    let strata = [age_stratum(), stratum("gender", "Gender", "/gender")];

    let result = Strata::try_parse(&strata, &request_with_context()).expect("paths resolve");

    assert!(matches!(result, Strata::Composite(ref v) if v.len() == 2));
}
