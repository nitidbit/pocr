//! Extracting identifiers from a randomization request.

use pocr_core::random::RandomizationRequest;
use serde_json::json;

fn request_with(context: serde_json::Value) -> RandomizationRequest {
    RandomizationRequest {
        locator: None,
        patient_id: None,
        encounter_id: None,
        user_id: None,
        fhir_server: None,
        context: Some(context),
    }
}

#[test]
fn context_retains_fhir_identifiers_without_prefetch() {
    let request = RandomizationRequest {
        locator: None,
        patient_id: Some("patient".into()),
        encounter_id: Some("encounter".into()),
        user_id: None,
        fhir_server: Some("https://synthetic.example/FHIR/R4".into()),
        context: None,
    };

    let context = request.extract_context();

    assert_eq!(
        serde_json::to_value(context).expect("serializable"),
        json!({
            "fhir_patient_id": "patient",
            "fhir_encounter_id": "encounter",
            "fhir_server": "https://synthetic.example/FHIR/R4"
        })
    );
}

#[test]
fn epic_ids_read_patient_external_identifier() {
    let request = request_with(json!({
        "patient": {
            "resourceType": "Patient",
            "identifier": [
                { "type": { "text": "EXTERNAL" },
                  "system": "urn:oid:1.2.840.114350.1.13.266.3.7.2.698084",
                  "value": "Z2134" }
            ]
        }
    }));

    let ids = request.extract_epic_ids();

    assert_eq!(ids.patient_id.as_deref(), Some("Z2134"));
}

#[test]
fn epic_ids_skip_internal_patient_identifier() {
    // INTERNAL shares the EXTERNAL system but carries a padded value; only
    // the EXTERNAL-typed entry must be selected.
    let request = request_with(json!({
        "patient": {
            "identifier": [
                { "type": { "text": "INTERNAL" },
                  "system": "urn:oid:1.2.840.114350.1.13.266.3.7.2.698084",
                  "value": "     Z2134" },
                { "type": { "text": "EXTERNAL" },
                  "system": "urn:oid:1.2.840.114350.1.13.266.3.7.2.698084",
                  "value": "Z2134" }
            ]
        }
    }));

    let ids = request.extract_epic_ids();

    assert_eq!(ids.patient_id.as_deref(), Some("Z2134"));
}

#[test]
fn epic_ids_read_csn_by_system_oid_suffix() {
    // The CSN system OID has variable segments; only the trailing
    // `.7.3.698084.8` is stable. A sibling identifier must be ignored.
    let request = request_with(json!({
        "encounter": {
            "resourceType": "Encounter",
            "identifier": [
                { "system": "urn:oid:1.2.840.114350.1.13.266.3.7.3.698084.7",
                  "value": "OTHER" },
                { "system": "urn:oid:1.2.840.114350.1.13.266.3.7.3.698084.8",
                  "value": "830100123" }
            ]
        }
    }));

    let ids = request.extract_epic_ids();

    assert_eq!(ids.csn.as_deref(), Some("830100123"));
}

#[test]
fn epic_ids_read_practitioner_external_identifier() {
    let request = request_with(json!({
        "user": {
            "resourceType": "Practitioner",
            "identifier": [
                { "type": { "text": "EXTERNAL" },
                  "system": "urn:oid:1.2.840.114350.1.13.266.3.7.2.697780",
                  "value": "1234" }
            ]
        }
    }));

    let ids = request.extract_epic_ids();

    assert_eq!(ids.user_id.as_deref(), Some("1234"));
}

#[test]
fn epic_ids_leave_absent_resources_none() {
    let request = request_with(json!({
        "patient": {
            "identifier": [
                { "type": { "text": "EXTERNAL" }, "value": "Z2134" }
            ]
        }
    }));

    let ids = request.extract_epic_ids();

    assert_eq!(ids.patient_id.as_deref(), Some("Z2134"));
    assert!(ids.csn.is_none());
    assert!(ids.user_id.is_none());
}

#[test]
fn epic_ids_are_all_none_without_context() {
    let request = RandomizationRequest {
        locator: None,
        patient_id: None,
        encounter_id: None,
        user_id: None,
        fhir_server: None,
        context: None,
    };

    let ids = request.extract_epic_ids();

    assert!(ids.patient_id.is_none());
    assert!(ids.csn.is_none());
    assert!(ids.user_id.is_none());
}
