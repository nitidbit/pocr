//! Small-cell suppression in `BoundReport::summarize`.

use pocr_core::dataset::{DatasetDefinition, DatasetRow, Report, ReportDefinition, ReportField};
use pocr_core::random::Arm;
use serde_json::{Value, json};
use uuid::Uuid;

const ARM_A: Uuid = Uuid::from_u128(0xa);
const ARM_B: Uuid = Uuid::from_u128(0xb);
const EPSILON: f64 = 1e-10;

fn summarize(smd: bool, variable: &Value, rows: &[DatasetRow]) -> Report {
    let dataset: DatasetDefinition = serde_json::from_value(json!({
        "version": 1, "name": "outcomes", "title": "Outcomes", "source": "caboodle",
        "stored_procedure": {"schema": "pocr", "name": "Outcomes"},
        "fields": [
            {"name": "los", "type": "number", "column": "LOS"},
            {"name": "day", "type": "string", "column": "DAY"}
        ]
    }))
    .expect("valid dataset");
    let report: ReportDefinition = serde_json::from_value(json!({
        "version": 1, "name": "report", "dataset": "outcomes", "title": "Report",
        "description": "By arm", "smd": smd, "variables": [variable]
    }))
    .expect("valid report");
    let arms = [ARM_A, ARM_B].map(|id| Arm {
        id,
        name: id.to_string(),
        ratio: 1,
        locator: None,
    });
    report
        .bind(&dataset)
        .expect("report binds to fixture dataset")
        .summarize(&arms, rows)
}

fn day(smd: bool, rows: &[DatasetRow]) -> Report {
    let variable = json!({"field": "day", "label": "Day", "analysis": "categorical"});
    summarize(smd, &variable, rows)
}

fn los(rows: &[DatasetRow]) -> Report {
    let variable = json!({"field": "los", "label": "LOS", "analysis": "continuous"});
    summarize(false, &variable, rows)
}

fn row(index: u128, arm_id: Uuid, values: Value) -> DatasetRow {
    DatasetRow {
        randomization_id: Uuid::from_u128(index),
        arm_id,
        arm_name: arm_id.to_string(),
        values: serde_json::from_value(values).expect("values"),
    }
}

/// `count` rows in `arm_id` holding `values`, numbered from `first`.
fn rows(first: u128, arm_id: Uuid, count: u128, values: &Value) -> Vec<DatasetRow> {
    (first..first + count)
        .map(|index| row(index, arm_id, values.clone()))
        .collect()
}

/// Rows of `day = value` per arm: `a` rows in arm A and `b` rows in arm B.
fn day_rows(value: &Value, a: u128, b: u128) -> Vec<DatasetRow> {
    let values = json!({"day": value});
    let mut all = rows(100, ARM_A, a, &values);
    all.extend(rows(200, ARM_B, b, &values));
    all
}

fn counts(report: &Report) -> Vec<Vec<Option<usize>>> {
    let ReportField::Categorical { categories, .. } = &report.fields[0] else {
        panic!("not categorical");
    };
    categories.iter().map(|c| c.counts.clone()).collect()
}

#[test]
fn summarize_suppresses_category_in_every_arm_when_any_arm_has_one_to_four() {
    let rows = day_rows(&json!("Monday"), 3, 20);

    assert_eq!(counts(&day(false, &rows)), [[None, None]]);
}

#[test]
fn summarize_shows_category_with_zero_or_five_rows() {
    let rows = day_rows(&json!("Monday"), 0, 5);

    assert_eq!(counts(&day(false, &rows)), [[Some(0), Some(5)]]);
}

#[test]
fn summarize_suppresses_missing_category_like_any_other() {
    let mut all = day_rows(&json!("Monday"), 10, 10);
    all.extend(day_rows(&Value::Null, 2, 9));

    let report = day(false, &all);

    assert_eq!(counts(&report), [[Some(10), Some(10)], [None, None]]);
}

#[test]
fn summarize_suppresses_continuous_arm_with_one_to_four_values() {
    let mut all = rows(1, ARM_A, 4, &json!({"los": 3.0}));
    all.extend(rows(10, ARM_B, 6, &json!({"los": 5.0})));

    let report = los(&all);

    let ReportField::Continuous {
        means,
        standard_deviations,
        denominators,
        ..
    } = &report.fields[0]
    else {
        panic!("not continuous");
    };
    assert_eq!(*means, [None, Some(5.0)]);
    assert_eq!(*standard_deviations, [None, Some(0.0)]);
    assert_eq!(*denominators, [None, Some(6)]);
}

#[test]
fn summarize_keeps_zero_denominator_for_arm_without_values() {
    let all = rows(1, ARM_A, 5, &json!({"los": 3.0}));

    let report = los(&all);

    let ReportField::Continuous { denominators, .. } = &report.fields[0] else {
        panic!("not continuous");
    };
    assert_eq!(*denominators, [Some(5), Some(0)]);
}

#[test]
fn summarize_computes_smd_from_suppressed_counts() {
    // Monday 3/10 vs 5/10 hides the category; the SMD still sees 0.3 vs 0.5.
    let mut all = rows(1, ARM_A, 3, &json!({"day": "Monday"}));
    all.extend(rows(10, ARM_A, 7, &json!({"day": "Tuesday"})));
    all.extend(rows(20, ARM_B, 5, &json!({"day": "Monday"})));
    all.extend(rows(30, ARM_B, 5, &json!({"day": "Tuesday"})));

    let report = day(true, &all);

    let ReportField::Categorical {
        categories,
        balance,
        ..
    } = &report.fields[0]
    else {
        panic!("not categorical");
    };
    assert_eq!(categories[0].counts, [None, None]);
    let pooled_variance = f64::midpoint(0.3 * 0.7, 0.5 * 0.5);
    let smd = balance.expect("report enables SMD").smd.expect("defined");
    assert!((smd - 0.2 / pooled_variance.sqrt()).abs() < EPSILON);
}

#[test]
fn summarize_never_suppresses_arm_counts() {
    let all = rows(1, ARM_A, 2, &json!({"day": "Monday"}));

    let report = day(false, &all);

    let sizes: Vec<_> = report.arms.iter().map(|arm| arm.count).collect();
    assert_eq!(sizes, [2, 0]);
}
