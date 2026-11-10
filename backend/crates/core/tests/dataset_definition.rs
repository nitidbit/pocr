//! Dataset definitions: fingerprints and stored-form decoding.

use pocr_core::dataset::DatasetDefinition;
use serde_json::{Value, json};

fn dataset_json() -> Value {
    json!({
        "version": 1,
        "name": "outcomes",
        "title": "Outcomes",
        "source": "caboodle",
        "stored_procedure": {"schema": "pocr", "name": "Outcomes"},
        "fields": [{"name": "los", "type": "number", "column": "LOS"}]
    })
}

fn definition(value: Value) -> DatasetDefinition {
    serde_json::from_value(value).expect("valid definition")
}

#[test]
fn fingerprint_survives_serialization_round_trip() {
    let original = definition(dataset_json());

    let encoded = serde_json::to_string(&original).expect("serializes");
    let restored: DatasetDefinition = serde_json::from_str(&encoded).expect("deserializes");

    assert_eq!(restored.fingerprint(), original.fingerprint());
    assert_eq!(restored, original);
}

#[test]
fn fingerprint_changes_when_definition_changes() {
    let mut changed = dataset_json();
    changed["title"] = json!("Changed");

    let changed = definition(changed);

    assert_ne!(
        changed.fingerprint(),
        definition(dataset_json()).fingerprint()
    );
}

#[test]
fn stored_definition_with_reserved_field_name_is_rejected() {
    let mut stored = dataset_json();
    stored["fields"][0]["name"] = json!("arm_id");

    let result = serde_json::from_value::<DatasetDefinition>(stored);

    assert!(result.is_err_and(|error| error.is_data()));
}
