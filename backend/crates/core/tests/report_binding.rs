//! Binding a report to a dataset definition, including a snapshot's captured one.

use pocr_core::dataset::{
    Analysis, DatasetDefinition, FieldType, IncompatibilityKind, IncompatibleReport,
    ReportDefinition,
};
use serde_json::{Value, json};

fn dataset_json() -> Value {
    json!({
        "version": 1,
        "name": "outcomes",
        "title": "Outcomes",
        "source": "caboodle",
        "stored_procedure": {"schema": "pocr", "name": "Outcomes"},
        "fields": [
            {"name": "los", "type": "number", "column": "LOS"},
            {"name": "readmitted", "type": "boolean", "column": "READMITTED"},
            {"name": "disposition", "type": "string", "column": "DISPOSITION"}
        ]
    })
}

fn report() -> ReportDefinition {
    serde_json::from_value(json!({
        "version": 1,
        "name": "outcomes",
        "dataset": "outcomes",
        "title": "Outcomes",
        "description": "By arm",
        "variables": [
            {
                "field": "los", "label": "LOS", "analysis": "continuous",
                "include_when": {"field": "readmitted", "equals": true}
            },
            {"field": "disposition", "label": "Disposition", "analysis": "categorical"}
        ]
    }))
    .expect("valid report")
}

fn categorical_report_json() -> Value {
    json!({
        "version": 1,
        "name": "outcomes",
        "dataset": "outcomes",
        "title": "Outcomes",
        "description": "By arm",
        "variables": [
            {"field": "disposition", "label": "Disposition", "analysis": "categorical"}
        ]
    })
}

fn captured(edit: impl FnOnce(&mut Value)) -> DatasetDefinition {
    let mut value = dataset_json();
    edit(&mut value);
    serde_json::from_value(value).expect("valid dataset")
}

fn bind_error(dataset: &DatasetDefinition) -> IncompatibleReport {
    report().bind(dataset).expect_err("report should not bind")
}

#[test]
fn matching_definition_binds() {
    let dataset = captured(|_| {});
    let report = report();

    let bound = report.bind(&dataset).expect("binds");

    assert_eq!(bound.definition().name().as_ref(), "outcomes");
}

#[test]
fn unknown_variable_field_is_incompatible() {
    let error = bind_error(&captured(|v| v["fields"][2]["name"] = json!("destination")));

    assert!(matches!(
        &error,
        IncompatibleReport::UnknownField { field } if field.as_ref() == "disposition"
    ));
    assert_eq!(error.field().as_ref(), "disposition");
    assert_eq!(error.kind(), IncompatibilityKind::UnknownField);
}

#[test]
fn unknown_condition_field_names_the_condition_field() {
    let error = bind_error(&captured(|v| v["fields"][1]["name"] = json!("returned")));

    assert!(matches!(
        &error,
        IncompatibleReport::UnknownField { field } if field.as_ref() == "readmitted"
    ));
    assert_eq!(error.field().as_ref(), "readmitted");
    assert_eq!(error.kind(), IncompatibilityKind::UnknownField);
}

#[test]
fn continuous_analysis_of_a_field_changed_to_string_is_incompatible() {
    let error = bind_error(&captured(|v| v["fields"][0]["type"] = json!("string")));

    assert!(matches!(
        &error,
        IncompatibleReport::IncompatibleAnalysis {
            analysis: Analysis::Continuous,
            kind: FieldType::String,
            ..
        }
    ));
    assert_eq!(error.field().as_ref(), "los");
    assert_eq!(error.kind(), IncompatibilityKind::IncompatibleType);
}

#[test]
fn category_order_on_a_field_changed_to_boolean_is_incompatible() {
    let mut value = categorical_report_json();
    value["variables"][0]["category_order"] = json!(["home"]);
    let ordered: ReportDefinition = serde_json::from_value(value).expect("valid report");
    let dataset = captured(|v| v["fields"][2]["type"] = json!("boolean"));

    let error = ordered.bind(&dataset).expect_err("should not bind");

    assert!(matches!(
        &error,
        IncompatibleReport::CategoryOrderOnBoolean { field } if field.as_ref() == "disposition"
    ));
    assert_eq!(error.field().as_ref(), "disposition");
    assert_eq!(error.kind(), IncompatibilityKind::IncompatibleType);
}

#[test]
fn condition_field_type_change_is_incompatible() {
    let error = bind_error(&captured(|v| v["fields"][1]["type"] = json!("string")));

    assert!(matches!(
        &error,
        IncompatibleReport::ConditionTypeMismatch {
            expected: FieldType::String,
            ..
        }
    ));
    assert_eq!(error.field().as_ref(), "readmitted");
    assert_eq!(error.kind(), IncompatibilityKind::IncompatibleType);
}

#[test]
fn captured_definition_with_extra_fields_binds() {
    let dataset = captured(|v| {
        v["fields"]
            .as_array_mut()
            .expect("fields")
            .push(json!({"name": "extra", "type": "string", "column": "EXTRA"}));
    });

    assert!(report().bind(&dataset).is_ok());
}

#[test]
#[should_panic(expected = "must bind to its own dataset")]
fn binding_against_another_datasets_definition_panics() {
    let other = captured(|v| v["name"] = json!("other"));

    let _ = report().bind(&other);
}
