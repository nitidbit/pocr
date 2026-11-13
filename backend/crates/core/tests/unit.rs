//! Resolving randomization units from requests.

use pocr_core::error::RandomizationError;
use pocr_core::random::{FhirPath, KeyVariant, RandomizationRequest, SimpleUnit, Unit};

fn empty_request() -> RandomizationRequest {
    RandomizationRequest {
        locator: None,
        patient_id: None,
        encounter_id: None,
        user_id: None,
        fhir_server: None,
        context: None,
    }
}

#[test]
fn simple_unit_rejects_missing_patient_id() {
    let result = SimpleUnit::try_parse(&KeyVariant::PatientId, &empty_request());

    assert!(matches!(
        result,
        Err(RandomizationError::MissingField {
            field: "patient_id"
        })
    ));
}

#[test]
fn simple_unit_rejects_missing_encounter_id() {
    let result = SimpleUnit::try_parse(&KeyVariant::EncounterId, &empty_request());

    assert!(matches!(
        result,
        Err(RandomizationError::MissingField {
            field: "encounter_id"
        })
    ));
}

#[test]
fn simple_unit_rejects_missing_user_id() {
    let result = SimpleUnit::try_parse(&KeyVariant::ProviderId, &empty_request());

    assert!(matches!(
        result,
        Err(RandomizationError::MissingField { field: "user_id" })
    ));
}

#[test]
fn simple_unit_reads_present_patient_id() {
    let request = RandomizationRequest {
        patient_id: Some("P-001".to_owned()),
        ..empty_request()
    };

    let result = SimpleUnit::try_parse(&KeyVariant::PatientId, &request);

    assert_eq!(
        result.expect("patient id present"),
        SimpleUnit::PatientId("P-001".to_owned())
    );
}

#[test]
fn simple_unit_rejects_fhir_path_key_as_not_implemented() {
    let key = KeyVariant::FhirPath(FhirPath {
        id: "fp-1".to_owned(),
        name: "test".to_owned(),
        context: "Patient".to_owned(),
        path: "/age".to_owned(),
    });

    let result = SimpleUnit::try_parse(&key, &empty_request());

    assert!(matches!(result, Err(RandomizationError::NotImplemented(_))));
}

#[test]
fn unit_rejects_empty_keys_as_not_found() {
    let result = Unit::try_parse(&[], &empty_request());

    assert!(matches!(result, Err(RandomizationError::NotFound)));
}

#[test]
fn unit_is_simple_for_single_key() {
    let request = RandomizationRequest {
        patient_id: Some("P-001".to_owned()),
        ..empty_request()
    };

    let result = Unit::try_parse(&[KeyVariant::PatientId], &request).expect("key present");

    assert!(matches!(result, Unit::Simple(SimpleUnit::PatientId(_))));
}

#[test]
fn unit_is_composite_for_multiple_keys() {
    let request = RandomizationRequest {
        patient_id: Some("P-001".to_owned()),
        encounter_id: Some("E-001".to_owned()),
        ..empty_request()
    };
    let keys = [KeyVariant::PatientId, KeyVariant::EncounterId];

    let result = Unit::try_parse(&keys, &request).expect("keys present");

    assert!(matches!(result, Unit::Composite(ref v) if v.len() == 2));
}
