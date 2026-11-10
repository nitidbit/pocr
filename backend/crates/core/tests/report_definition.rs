//! Report definitions: identity, naming, and accessors.

use pocr_core::dataset::{DefinitionError, ReportDefinition, ReportName};
use serde_json::{Value, json};

fn report_json() -> Value {
    json!({
        "version": 1,
        "name": "outcomes",
        "dataset": "outcomes-data",
        "title": "Outcomes",
        "description": "By arm",
        "variables": [{"field": "los", "label": "LOS", "analysis": "continuous"}]
    })
}

#[test]
fn valid_report_exposes_its_identity() {
    let report: ReportDefinition = serde_json::from_value(report_json()).expect("valid report");

    assert_eq!(report.name().as_str(), "outcomes");
    assert_eq!(report.dataset().as_str(), "outcomes-data");
    assert_eq!(report.variables().len(), 1);
}

#[test]
fn report_name_rejects_invalid_names() {
    for bad in ["", "Bad_Name", "has space", &"a".repeat(129)] {
        let error = ReportName::parse(bad).expect_err("invalid name");

        assert!(matches!(
            error,
            DefinitionError::InvalidReportName { value } if value == bad
        ));
    }
}

#[test]
fn report_name_accepts_lowercase_digits_and_hyphens() {
    assert!(ReportName::parse("a-1").is_ok());
}

#[test]
fn report_with_invalid_dataset_reference_is_rejected() {
    let mut value = report_json();
    value["dataset"] = json!("Bad Name");

    let result = serde_json::from_value::<ReportDefinition>(value);

    assert!(result.is_err_and(|error| error.is_data()));
}

#[test]
fn report_without_dataset_reference_is_rejected() {
    let mut value = report_json();
    value.as_object_mut().expect("object").remove("dataset");

    let result = serde_json::from_value::<ReportDefinition>(value);

    assert!(result.is_err_and(|error| error.is_data()));
}
