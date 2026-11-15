//! Standardized mean difference in `BoundReport::summarize`.

use pocr_core::dataset::{
    Balance, DatasetDefinition, DatasetRow, Report, ReportDefinition, ReportField,
};
use pocr_core::random::Arm;
use serde_json::{Value, json};
use uuid::Uuid;

const ARM_A: Uuid = Uuid::from_u128(0xa);
const ARM_B: Uuid = Uuid::from_u128(0xb);
const ARM_C: Uuid = Uuid::from_u128(0xc);
const EPSILON: f64 = 1e-10;

fn summarize(smd: bool, variable: &Value, arms: &[Uuid], rows: &[DatasetRow]) -> Report {
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
    let arms: Vec<Arm> = arms
        .iter()
        .map(|&id| Arm {
            id,
            name: id.to_string(),
            ratio: 1,
            locator: None,
        })
        .collect();
    report
        .bind(&dataset)
        .expect("report binds to fixture dataset")
        .summarize(&arms, rows)
}

fn los(smd: bool, arms: &[Uuid], rows: &[DatasetRow]) -> Report {
    let variable = json!({"field": "los", "label": "LOS", "analysis": "continuous"});
    summarize(smd, &variable, arms, rows)
}

fn day(smd: bool, arms: &[Uuid], rows: &[DatasetRow]) -> Report {
    let variable = json!({"field": "day", "label": "Day", "analysis": "categorical"});
    summarize(smd, &variable, arms, rows)
}

/// One row per entry of `values` in `arm_id`, numbered from `first`.
fn los_rows(first: u128, arm_id: Uuid, values: [f64; 5]) -> Vec<DatasetRow> {
    (first..)
        .zip(values)
        .map(|(index, value)| row(index, arm_id, json!({"los": value})))
        .collect()
}

/// `count` rows in `arm_id` with `day` set to `value`, numbered from `first`.
fn day_rows(first: u128, arm_id: Uuid, count: u128, value: &Value) -> Vec<DatasetRow> {
    (first..first + count)
        .map(|index| row(index, arm_id, json!({"day": value})))
        .collect()
}

fn row(index: u128, arm_id: Uuid, values: Value) -> DatasetRow {
    DatasetRow {
        randomization_id: Uuid::from_u128(index),
        arm_id,
        arm_name: arm_id.to_string(),
        values: serde_json::from_value(values).expect("values"),
    }
}

fn balance(report: &Report) -> Option<Balance> {
    match &report.fields[0] {
        ReportField::Categorical { balance, .. } | ReportField::Continuous { balance, .. } => {
            *balance
        }
    }
}

fn smd_of(report: &Report) -> Option<f64> {
    balance(report).expect("report enables SMD").smd
}

/// Five values with mean `base + 4` and sample variance 10.
fn spread(base: f64) -> [f64; 5] {
    [base, base + 2.0, base + 4.0, base + 6.0, base + 8.0]
}

fn assert_close(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("SMD is defined");
    assert!(
        (actual - expected).abs() < EPSILON,
        "{actual} != {expected}"
    );
}

#[test]
fn summarize_omits_balance_when_report_disables_smd() {
    let mut rows = los_rows(1, ARM_A, spread(10.0));
    rows.extend(los_rows(10, ARM_B, spread(20.0)));

    let report = los(false, &[ARM_A, ARM_B], &rows);

    assert!(!report.smd);
    assert_eq!(balance(&report), None);
    let field = &serde_json::to_value(&report).expect("serializes")["fields"][0];
    assert!(field.get("smd").is_none());
    assert!(field.get("imbalanced").is_none());
}

#[test]
fn summarize_reports_continuous_smd_from_arm_means_and_sds() {
    let mut rows = los_rows(1, ARM_A, spread(10.0));
    rows.extend(los_rows(10, ARM_B, spread(20.0)));

    let report = los(true, &[ARM_A, ARM_B], &rows);

    // Means 14 and 24, both variances 10: 10 / sqrt(10).
    assert!(report.smd);
    assert_close(smd_of(&report), 10.0 / 10.0_f64.sqrt());
}

#[test]
fn summarize_counts_missing_as_a_categorical_level() {
    let mut rows = day_rows(1, ARM_A, 10, &json!("Monday"));
    rows.extend(day_rows(20, ARM_B, 5, &json!("Monday")));
    rows.extend(day_rows(30, ARM_B, 5, &Value::Null));

    let report = day(true, &[ARM_A, ARM_B], &rows);

    // Monday 1.0 vs 0.5 and Missing 0.0 vs 0.5 give the same pair SMD.
    assert_close(smd_of(&report), 0.5 / 0.125_f64.sqrt());
}

#[test]
fn summarize_takes_largest_pair_across_three_arms() {
    let mut rows = los_rows(1, ARM_A, spread(10.0));
    rows.extend(los_rows(10, ARM_B, spread(20.0)));
    rows.extend(los_rows(20, ARM_C, spread(50.0)));

    let report = los(true, &[ARM_A, ARM_B, ARM_C], &rows);

    assert_close(smd_of(&report), 40.0 / 10.0_f64.sqrt());
}

#[test]
fn summarize_excludes_configured_arm_without_rows_from_smd() {
    let mut rows = los_rows(1, ARM_A, spread(10.0));
    rows.extend(los_rows(10, ARM_C, spread(20.0)));

    let report = los(true, &[ARM_A, ARM_B, ARM_C], &rows);

    assert_close(smd_of(&report), 10.0 / 10.0_f64.sqrt());
}

#[test]
fn summarize_reports_null_smd_when_one_arm_has_data() {
    let rows = los_rows(1, ARM_A, spread(10.0));

    let report = los(true, &[ARM_A, ARM_B], &rows);

    let balance = balance(&report).expect("report enables SMD");
    assert_eq!((balance.smd, balance.imbalanced), (None, None));
}

#[test]
fn summarize_reports_null_smd_when_an_arm_has_a_single_value() {
    let mut rows = los_rows(1, ARM_A, spread(10.0));
    rows.push(row(10, ARM_B, json!({"los": 3.0})));

    let report = los(true, &[ARM_A, ARM_B], &rows);

    assert_eq!(smd_of(&report), None);
}

#[test]
fn summarize_reports_null_smd_when_pooled_variance_is_zero() {
    let mut rows = day_rows(1, ARM_A, 10, &json!("Monday"));
    rows.extend(day_rows(20, ARM_B, 10, &json!("Monday")));

    let report = day(true, &[ARM_A, ARM_B], &rows);

    let balance = balance(&report).expect("report enables SMD");
    assert_eq!((balance.smd, balance.imbalanced), (None, None));
}

#[test]
fn summarize_flags_imbalance_above_threshold() {
    let mut rows = los_rows(1, ARM_A, spread(10.0));
    rows.extend(los_rows(10, ARM_B, spread(20.0)));

    let report = los(true, &[ARM_A, ARM_B], &rows);

    assert_eq!(balance(&report).expect("SMD").imbalanced, Some(true));
}

#[test]
fn summarize_does_not_flag_balanced_field() {
    let mut rows = los_rows(1, ARM_A, spread(10.0));
    rows.extend(los_rows(10, ARM_B, spread(10.0)));

    let report = los(true, &[ARM_A, ARM_B], &rows);

    assert_eq!(balance(&report).expect("SMD").imbalanced, Some(false));
}
