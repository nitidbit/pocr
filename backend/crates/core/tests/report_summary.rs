//! Arm-grouped report summaries produced by `BoundReport::summarize`.

use pocr_core::dataset::{
    Category, DatasetDefinition, DatasetRow, Report, ReportDefinition, ReportField,
};
use pocr_core::random::Arm;
use serde_json::{Value, json};
use uuid::Uuid;

const ARM_A: Uuid = Uuid::from_u128(0xa);
const ARM_B: Uuid = Uuid::from_u128(0xb);
const ARM_C: Uuid = Uuid::from_u128(0xc);

fn dataset() -> DatasetDefinition {
    serde_json::from_value(json!({
        "version": 1, "name": "outcomes", "title": "Outcomes", "source": "caboodle",
        "stored_procedure": {"schema": "pocr", "name": "Outcomes"},
        "fields": [
            {"name": "los", "type": "number", "column": "LOS"},
            {"name": "readmitted", "type": "boolean", "column": "READMITTED"},
            {"name": "day", "type": "string", "column": "DAY"},
            {"name": "days_to_readmit", "type": "number", "column": "DAYS"}
        ]
    }))
    .expect("valid dataset")
}

/// A report definition that binds to the fixture dataset on every summary.
struct Fixture(ReportDefinition);

impl Fixture {
    fn summarize(&self, arms: &[Arm], rows: &[DatasetRow]) -> Report {
        self.0
            .bind(&dataset())
            .expect("report binds to fixture dataset")
            .summarize(arms, rows)
    }
}

fn report(variables: Value) -> Fixture {
    Fixture(
        serde_json::from_value(json!({
            "version": 1, "name": "report", "dataset": "outcomes", "title": "Report",
            "description": "By arm", "variables": variables
        }))
        .expect("valid report"),
    )
}

fn arm(id: Uuid, name: &str) -> Arm {
    Arm {
        id,
        name: name.into(),
        ratio: 1,
        locator: None,
    }
}

fn row(index: u128, arm_id: Uuid, values: Value) -> DatasetRow {
    DatasetRow {
        randomization_id: Uuid::from_u128(index),
        arm_id,
        arm_name: format!("stored-{arm_id}"),
        values: serde_json::from_value(values).expect("values"),
    }
}

fn categorical(report: &Report, index: usize) -> &[Category] {
    let ReportField::Categorical { categories, .. } = &report.fields[index] else {
        panic!("field {index} is not categorical");
    };
    categories
}

fn continuous(report: &Report, index: usize) -> (&[Option<f64>], &[Option<f64>], &[usize]) {
    let ReportField::Continuous {
        means,
        standard_deviations,
        denominators,
        ..
    } = &report.fields[index]
    else {
        panic!("field {index} is not continuous");
    };
    (means, standard_deviations, denominators)
}

fn day_report() -> Fixture {
    report(json!([{
        "field": "day", "label": "Day", "analysis": "categorical",
        "category_order": ["Monday", "Tuesday"]
    }]))
}

fn los_report() -> Fixture {
    report(json!([{"field": "los", "label": "LOS", "analysis": "continuous"}]))
}

/// A report of `days_to_readmit` restricted to rows where `los` equals `equals`.
fn readmit_when_los(equals: Value) -> Fixture {
    report(json!([{
        "field": "days_to_readmit", "label": "Days to readmission", "analysis": "continuous",
        "include_when": {"field": "los", "equals": equals}
    }]))
}

#[test]
fn summarize_includes_configured_arm_without_rows() {
    let rows = [row(1, ARM_A, json!({"day": "Monday"}))];

    let summary = day_report().summarize(&[arm(ARM_A, "A"), arm(ARM_B, "B")], &rows);

    let counts: Vec<_> = summary.arms.iter().map(|a| a.count).collect();
    assert_eq!(counts, [1, 0]);
    assert_eq!(summary.arms[1].name, "B");
    assert_eq!(categorical(&summary, 0)[0].counts, [1, 0]);
}

#[test]
fn summarize_names_arms_from_configuration_not_stored_rows() {
    let rows = [row(1, ARM_A, json!({"day": "Monday"}))];

    let summary = day_report().summarize(&[arm(ARM_A, "Renamed")], &rows);

    assert_eq!(summary.arms[0].name, "Renamed");
}

#[test]
fn summarize_appends_unconfigured_arms_in_first_row_order() {
    let rows = [
        row(3, ARM_B, json!({"day": "Monday"})),
        row(2, ARM_C, json!({"day": "Monday"})),
        row(1, ARM_A, json!({"day": "Monday"})),
    ];

    let summary = day_report().summarize(&[arm(ARM_A, "A")], &rows);

    let ids: Vec<_> = summary.arms.iter().map(|a| a.id).collect();
    assert_eq!(ids, [ARM_A, ARM_C, ARM_B]);
    assert_eq!(summary.arms[1].name, format!("stored-{ARM_C}"));
}

#[test]
fn summarize_orders_listed_categories_then_unlisted_lexically_then_missing() {
    let rows = [
        row(1, ARM_A, json!({"day": "Sunday"})),
        row(2, ARM_A, json!({"day": "Tuesday"})),
        row(3, ARM_A, json!({"day": null})),
        row(4, ARM_A, json!({"day": "Friday"})),
        row(5, ARM_A, json!({"day": "Monday"})),
    ];

    let summary = day_report().summarize(&[arm(ARM_A, "A")], &rows);

    let ids: Vec<_> = categorical(&summary, 0)
        .iter()
        .map(|c| c.id.as_deref())
        .collect();
    assert_eq!(
        ids,
        [
            Some("Monday"),
            Some("Tuesday"),
            Some("Friday"),
            Some("Sunday"),
            None
        ]
    );
}

#[test]
fn summarize_labels_missing_category_distinct_from_a_real_missing_value() {
    let rows = [
        row(1, ARM_A, json!({"day": "Missing"})),
        row(2, ARM_A, json!({"day": null})),
    ];

    let summary = day_report().summarize(&[arm(ARM_A, "A")], &rows);

    let categories = categorical(&summary, 0);
    assert_eq!(categories.len(), 2);
    assert_eq!(categories[0].id.as_deref(), Some("Missing"));
    assert_eq!(categories[1].id, None);
    assert_eq!(categories[1].label, "Missing");
}

#[test]
fn summarize_omits_missing_category_when_every_row_has_a_value() {
    let rows = [row(1, ARM_A, json!({"day": "Monday"}))];

    let summary = day_report().summarize(&[arm(ARM_A, "A")], &rows);

    assert!(categorical(&summary, 0).iter().all(|c| c.id.is_some()));
}

#[test]
fn summarize_labels_booleans_yes_no_in_true_false_missing_order() {
    let report = report(json!([{
        "field": "readmitted", "label": "Readmission", "analysis": "categorical"
    }]));
    let rows = [
        row(1, ARM_A, json!({"readmitted": false})),
        row(2, ARM_A, json!({"readmitted": null})),
        row(3, ARM_A, json!({"readmitted": true})),
    ];

    let summary = report.summarize(&[arm(ARM_A, "A")], &rows);

    let labels: Vec<_> = categorical(&summary, 0)
        .iter()
        .map(|c| (c.id.as_deref(), c.label.as_str()))
        .collect();
    assert_eq!(
        labels,
        [
            (Some("true"), "Yes"),
            (Some("false"), "No"),
            (None, "Missing")
        ]
    );
}

#[test]
fn summarize_averages_only_eligible_nonnull_values() {
    let report = report(json!([{
        "field": "days_to_readmit", "label": "Readmission", "analysis": "continuous",
        "unit": "days", "include_when": {"field": "readmitted", "equals": true}
    }]));
    let rows = [
        row(
            1,
            ARM_A,
            json!({"readmitted": true, "days_to_readmit": 2.0}),
        ),
        row(
            2,
            ARM_A,
            json!({"readmitted": true, "days_to_readmit": 4.0}),
        ),
        row(
            3,
            ARM_A,
            json!({"readmitted": true, "days_to_readmit": null}),
        ),
        row(
            4,
            ARM_A,
            json!({"readmitted": false, "days_to_readmit": 100.0}),
        ),
    ];

    let summary = report.summarize(&[arm(ARM_A, "A")], &rows);

    let (means, _, denominators) = continuous(&summary, 0);
    assert_eq!(summary.arms[0].count, 4);
    assert_eq!(means, [Some(3.0)]);
    assert_eq!(denominators, [2]);
}

#[test]
fn summarize_reports_sample_standard_deviation() {
    let rows = [
        row(1, ARM_A, json!({"los": 1.0})),
        row(2, ARM_A, json!({"los": 2.0})),
        row(3, ARM_A, json!({"los": 3.0})),
    ];

    let summary = los_report().summarize(&[arm(ARM_A, "A")], &rows);

    let (_, sds, _) = continuous(&summary, 0);
    assert_eq!(sds, [Some(1.0)]);
}

#[test]
fn summarize_reports_no_standard_deviation_for_a_single_value() {
    let rows = [row(1, ARM_A, json!({"los": 3.0}))];

    let summary = los_report().summarize(&[arm(ARM_A, "A")], &rows);

    let (means, sds, denominators) = continuous(&summary, 0);
    assert_eq!(means, [Some(3.0)]);
    assert_eq!(sds, [None]);
    assert_eq!(denominators, [1]);
}

#[test]
fn summarize_gives_empty_arm_no_mean_and_zero_denominator() {
    let rows = [row(1, ARM_A, json!({"los": 3.0}))];

    let summary = los_report().summarize(&[arm(ARM_A, "A"), arm(ARM_B, "B")], &rows);

    let (means, _, denominators) = continuous(&summary, 0);
    assert_eq!(means, [Some(3.0), None]);
    assert_eq!(denominators, [1, 0]);
}

#[test]
fn summarize_matches_float_value_against_integer_condition() {
    let rows = [row(1, ARM_A, json!({"los": 1.0, "days_to_readmit": 5.0}))];

    let summary = readmit_when_los(json!(1)).summarize(&[arm(ARM_A, "A")], &rows);

    assert_eq!(continuous(&summary, 0).2, [1]);
}

#[test]
fn summarize_matches_integer_value_against_float_condition() {
    let rows = [row(1, ARM_A, json!({"los": 1, "days_to_readmit": 5.0}))];

    let summary = readmit_when_los(json!(1.0)).summarize(&[arm(ARM_A, "A")], &rows);

    assert_eq!(continuous(&summary, 0).2, [1]);
}

#[test]
fn summarize_excludes_fractional_value_from_integer_condition() {
    let rows = [row(1, ARM_A, json!({"los": 1.1, "days_to_readmit": 5.0}))];

    let summary = readmit_when_los(json!(1)).summarize(&[arm(ARM_A, "A")], &rows);

    assert_eq!(continuous(&summary, 0).2, [0]);
}

#[test]
fn summarize_excludes_integer_beyond_float_precision_from_nearest_float_condition() {
    let rows = [row(
        1,
        ARM_A,
        json!({"los": 9_007_199_254_740_993_u64, "days_to_readmit": 5.0}),
    )];

    let summary =
        readmit_when_los(json!(9_007_199_254_740_992.0)).summarize(&[arm(ARM_A, "A")], &rows);

    assert_eq!(continuous(&summary, 0).2, [0]);
}

#[test]
fn summarize_serializes_fields_with_an_analysis_tag() {
    let report = report(json!([
        {"field": "los", "label": "LOS", "analysis": "continuous"},
        {"field": "day", "label": "Day", "analysis": "categorical"}
    ]));

    let encoded =
        serde_json::to_value(report.summarize(&[arm(ARM_A, "A")], &[])).expect("serializes");

    assert_eq!(encoded["fields"][0]["analysis"], json!("continuous"));
    assert_eq!(encoded["fields"][1]["analysis"], json!("categorical"));
}

#[test]
fn summarize_serializes_field_name_as_id_and_label_as_name() {
    let encoded =
        serde_json::to_value(los_report().summarize(&[arm(ARM_A, "A")], &[])).expect("serializes");

    assert_eq!(encoded["fields"][0]["id"], json!("los"));
    assert_eq!(encoded["fields"][0]["name"], json!("LOS"));
}
