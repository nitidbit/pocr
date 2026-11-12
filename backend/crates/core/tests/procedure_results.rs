//! Procedure results validated against the definition and the input cohort.

use pocr_core::dataset::{ContextRecord, DatasetDefinition, FieldType, ResultRow, ResultsError};
use serde_json::{Value, json};
use uuid::Uuid;

const ID_A: Uuid = Uuid::from_u128(1);
const ID_B: Uuid = Uuid::from_u128(2);

fn definition() -> DatasetDefinition {
    serde_json::from_value(json!({
        "version": 1,
        "name": "outcomes",
        "title": "Outcomes",
        "source": "caboodle",
        "stored_procedure": {"schema": "pocr", "name": "Outcomes"},
        "fields": [
            {"name": "los", "type": "number", "column": "LOS"},
            {"name": "readmission", "type": "boolean", "column": "READMIT_IND"},
            {"name": "disposition", "type": "string", "column": "DISPOSITION"}
        ]
    }))
    .expect("valid dataset")
}

fn record(id: Uuid) -> ContextRecord {
    ContextRecord {
        randomization_id: id,
        randomization_time: "2026-10-01T14:30:00Z".parse().expect("fixed timestamp"),
        context: Default::default(),
    }
}

fn null_row(id: Uuid) -> ResultRow {
    ResultRow::from([
        ("randomization_id".into(), json!(id)),
        ("LOS".into(), Value::Null),
        ("READMIT_IND".into(), Value::Null),
        ("DISPOSITION".into(), Value::Null),
    ])
}

#[test]
fn parse_results_keys_values_by_field_name_and_accepts_nulls() {
    let definition = definition();

    let records = definition
        .parse_results(&[record(ID_A)], vec![null_row(ID_A)])
        .expect("nulls are accepted");

    let keys: Vec<&str> = records[0].values().keys().map(|k| k.as_ref()).collect();
    assert_eq!(records[0].randomization_id(), ID_A);
    assert_eq!(keys, ["disposition", "los", "readmission"]);
}

#[test]
fn parse_results_rejects_repeated_input_id() {
    let definition = definition();

    let result = definition.parse_results(
        &[record(ID_A), record(ID_A)],
        vec![null_row(ID_A), null_row(ID_A)],
    );

    assert!(matches!(
        result,
        Err(ResultsError::DuplicateInput { randomization_id }) if randomization_id == ID_A
    ));
}

#[test]
fn parse_results_rejects_missing_rows() {
    let definition = definition();

    let result = definition.parse_results(&[record(ID_A)], vec![]);

    assert!(matches!(
        result,
        Err(ResultsError::RowCount {
            expected: 1,
            actual: 0
        })
    ));
}

#[test]
fn parse_results_rejects_extra_column() {
    let definition = definition();
    let mut row = null_row(ID_A);
    row.insert("EXTRA".into(), Value::Null);

    let result = definition.parse_results(&[record(ID_A)], vec![row]);

    assert!(matches!(
        result,
        Err(ResultsError::ColumnSet { missing, unexpected })
            if missing.is_empty() && unexpected == ["EXTRA"]
    ));
}

#[test]
fn parse_results_names_missing_column() {
    let definition = definition();
    let mut row = null_row(ID_A);
    row.remove("LOS");
    row.insert("EXTRA".into(), Value::Null);

    let result = definition.parse_results(&[record(ID_A)], vec![row]);

    assert!(matches!(
        result,
        Err(ResultsError::ColumnSet { missing, .. })
            if missing.len() == 1 && missing[0].as_ref() == "LOS"
    ));
}

#[test]
fn parse_results_rejects_row_without_id() {
    let definition = definition();
    let mut row = null_row(ID_A);
    row.remove("randomization_id");

    let result = definition.parse_results(&[record(ID_A)], vec![row]);

    assert!(matches!(result, Err(ResultsError::MissingRowId)));
}

#[test]
fn parse_results_rejects_non_uuid_row_id() {
    let definition = definition();
    let mut row = null_row(ID_A);
    row.insert("randomization_id".into(), json!("not-a-uuid"));

    let result = definition.parse_results(&[record(ID_A)], vec![row]);

    assert!(matches!(result, Err(ResultsError::InvalidRowId)));
}

#[test]
fn parse_results_rejects_row_id_outside_input() {
    let definition = definition();

    let result = definition.parse_results(&[record(ID_A)], vec![null_row(ID_B)]);

    assert!(matches!(
        result,
        Err(ResultsError::UnexpectedRowId { randomization_id }) if randomization_id == ID_B
    ));
}

#[test]
fn parse_results_rejects_repeated_row_id() {
    let definition = definition();

    let result = definition.parse_results(
        &[record(ID_A), record(ID_B)],
        vec![null_row(ID_A), null_row(ID_A)],
    );

    assert!(matches!(
        result,
        Err(ResultsError::DuplicateRowId { randomization_id }) if randomization_id == ID_A
    ));
}

#[test]
fn parse_results_rejects_value_of_wrong_type() {
    let definition = definition();
    let mut row = null_row(ID_A);
    row.insert("READMIT_IND".into(), json!(1));

    let result = definition.parse_results(&[record(ID_A)], vec![row]);

    assert!(matches!(
        result,
        Err(ResultsError::TypeMismatch { field, expected: FieldType::Boolean })
            if field.as_ref() == "readmission"
    ));
}

#[test]
fn type_mismatch_error_omits_the_offending_value() {
    let definition = definition();
    let mut row = null_row(ID_A);
    row.insert("LOS".into(), json!("sensitive"));

    let error = definition
        .parse_results(&[record(ID_A)], vec![row])
        .expect_err("string is not a number");

    assert!(!error.to_string().contains("sensitive"));
    assert!(!format!("{error:?}").contains("sensitive"));
}

#[test]
fn check_columns_rejects_repeated_result_column() {
    let definition = definition();
    let columns = [
        "randomization_id",
        "LOS",
        "READMIT_IND",
        "DISPOSITION",
        "LOS",
    ];

    let result = definition.check_columns(columns);

    assert!(matches!(
        result,
        Err(ResultsError::ColumnSet { missing, unexpected })
            if missing.is_empty() && unexpected == ["LOS"]
    ));
}
