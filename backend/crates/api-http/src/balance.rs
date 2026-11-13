//! Balance endpoint for demographic parity assessment across treatment arms.
//!
//! Computes per-variable SMD (Standardized Mean Difference) across arms and
//! applies small-cell suppression for patient privacy. Used by the frontend
//! to display a balance table for study investigators.

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{NaiveDate, Utc};
use serde::Serialize;
use tracing::instrument;
use uuid::Uuid;

use pocr_core::demographics::PatientDemographicsWithArm;

use crate::AppState;
use crate::smd::{ArmStats, smd_max_pairwise};

/// Minimum count threshold for cell reporting. Counts below this are redacted.
const SMALL_CELL_THRESHOLD: usize = 5;

// -- Response types --

/// Top-level balance response for a study.
#[derive(Debug, Serialize)]
pub struct BalanceResponse {
    pub study_id: Uuid,
    pub total_patients: usize,
    pub arms: Vec<ArmSummary>,
    pub variables: Vec<BalanceVariable>,
}

/// Summary of a single arm's enrollment.
#[derive(Debug, Serialize)]
pub struct ArmSummary {
    pub id: Uuid,
    pub count: usize,
}

/// Balance data for a single demographic variable.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum BalanceVariable {
    #[serde(rename = "continuous")]
    Continuous {
        name: String,
        arms: Vec<ContinuousArmData>,
        smd: f64,
    },
    #[serde(rename = "categorical")]
    Categorical {
        name: String,
        categories: Vec<CategoryData>,
        smd: f64,
    },
}

/// Per-arm statistics for a continuous variable.
#[derive(Debug, Serialize)]
pub struct ContinuousArmData {
    pub arm_id: Uuid,
    pub mean: f64,
    pub sd: f64,
    pub missing: usize,
}

/// One category level for a categorical variable.
#[derive(Debug, Serialize)]
pub struct CategoryData {
    pub value: String,
    pub arms: Vec<CategoricalArmData>,
}

/// Per-arm count/proportion for a single category value.
#[derive(Debug, Serialize)]
pub struct CategoricalArmData {
    pub arm_id: Uuid,
    pub count: serde_json::Value,
    pub pct: serde_json::Value,
}

// -- Handler --

/// Retrieves demographic balance data for a study.
///
/// Fetches demographics and arm records, aggregates per-arm statistics, computes
/// the SMD of each variable, and applies small-cell suppression.
///
/// Returns an empty `variables` array when no demographics have been synced.
#[instrument(name = "balance.get", skip(state), fields(%study_id))]
pub async fn get_balance(
    State(state): State<AppState>,
    Path(study_id): Path<Uuid>,
) -> Result<Json<BalanceResponse>, StatusCode> {
    let records = state
        .demographics_repo
        .find_by_study(&study_id)
        .await
        .map_err(|err| {
            tracing::error!(error = ?err, "demographics query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let today = Utc::now().date_naive();
    let response = build_balance_response(study_id, &records, today);

    tracing::info!(
        study_id = %study_id,
        total_patients = response.total_patients,
        variable_count = response.variables.len(),
        "balance computed"
    );

    Ok(Json(response))
}

// -- Aggregation logic --

/// Builds the full balance response from raw demographics records.
fn build_balance_response(
    study_id: Uuid,
    records: &[PatientDemographicsWithArm],
    today: NaiveDate,
) -> BalanceResponse {
    let arm_ids = collect_arm_ids(records);

    let arms: Vec<ArmSummary> = arm_ids
        .iter()
        .map(|&arm_id| ArmSummary {
            id: arm_id,
            count: records.iter().filter(|r| r.arm_id == arm_id).count(),
        })
        .collect();

    let total_patients = records.len();

    let mut variables: Vec<BalanceVariable> = Vec::new();

    // Continuous: age (derived from birth_date), bmi
    if let Some(var) = build_continuous_variable("age", records, &arm_ids, |r| {
        r.birth_date.map(|bd| age_in_years(bd, today))
    }) {
        variables.push(var);
    }

    if let Some(var) = build_continuous_variable("bmi", records, &arm_ids, |r| r.bmi) {
        variables.push(var);
    }

    // Categorical: sex, ethnicity, primary_language, smoking_status, insurance_type
    if let Some(var) = build_categorical_variable("sex", records, &arm_ids, |r| {
        r.sex.as_deref().map(|s| vec![s.to_owned()])
    }) {
        variables.push(var);
    }

    if let Some(var) = build_categorical_variable("ethnicity", records, &arm_ids, |r| {
        r.ethnicity.as_deref().map(|s| vec![s.to_owned()])
    }) {
        variables.push(var);
    }

    if let Some(var) = build_categorical_variable("primary_language", records, &arm_ids, |r| {
        r.primary_language.as_deref().map(|s| vec![s.to_owned()])
    }) {
        variables.push(var);
    }

    if let Some(var) = build_categorical_variable("smoking_status", records, &arm_ids, |r| {
        r.smoking_status.as_deref().map(|s| vec![s.to_owned()])
    }) {
        variables.push(var);
    }

    if let Some(var) = build_categorical_variable("insurance_type", records, &arm_ids, |r| {
        r.insurance_type.as_deref().map(|s| vec![s.to_owned()])
    }) {
        variables.push(var);
    }

    // Race is multi-valued: each patient can have multiple race entries
    if let Some(var) = build_categorical_variable("race", records, &arm_ids, |r| r.races.clone()) {
        variables.push(var);
    }

    BalanceResponse {
        study_id,
        total_patients,
        arms,
        variables,
    }
}

/// Collects unique arm IDs from records, preserving insertion order.
fn collect_arm_ids(records: &[PatientDemographicsWithArm]) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for record in records {
        if seen.insert(record.arm_id) {
            ids.push(record.arm_id);
        }
    }
    ids
}

/// Computes age in years from a birth date to a reference date.
fn age_in_years(birth_date: NaiveDate, today: NaiveDate) -> f64 {
    let years = today.years_since(birth_date).unwrap_or(0);
    f64::from(years)
}

/// Builds a continuous variable (mean, SD, SMD) from an extractor function.
///
/// Returns `None` if no arm has any non-missing values.
fn build_continuous_variable(
    name: &str,
    records: &[PatientDemographicsWithArm],
    arm_ids: &[Uuid],
    extract: impl Fn(&PatientDemographicsWithArm) -> Option<f64>,
) -> Option<BalanceVariable> {
    let mut arm_data = Vec::with_capacity(arm_ids.len());
    let mut smd_stats = Vec::with_capacity(arm_ids.len());
    let mut any_values = false;

    for &arm_id in arm_ids {
        let arm_records: Vec<&PatientDemographicsWithArm> =
            records.iter().filter(|r| r.arm_id == arm_id).collect();

        let values: Vec<f64> = arm_records.iter().filter_map(|r| extract(r)).collect();
        let missing = arm_records.len() - values.len();

        let (mean, sd) = if values.is_empty() {
            (0.0, 0.0)
        } else {
            any_values = true;
            (compute_mean(&values), compute_sd(&values))
        };

        arm_data.push(ContinuousArmData {
            arm_id,
            mean,
            sd,
            missing,
        });
        smd_stats.push(ArmStats { value: mean, sd });
    }

    if !any_values {
        return None;
    }

    let smd = smd_max_pairwise(&smd_stats, true);

    Some(BalanceVariable::Continuous {
        name: name.to_owned(),
        arms: arm_data,
        smd,
    })
}

/// Builds a categorical variable with per-level SMD and small-cell suppression.
///
/// Returns `None` if no values exist across all arms.
fn build_categorical_variable(
    name: &str,
    records: &[PatientDemographicsWithArm],
    arm_ids: &[Uuid],
    extract: impl Fn(&PatientDemographicsWithArm) -> Option<Vec<String>>,
) -> Option<BalanceVariable> {
    // Collect all distinct category values and per-arm counts.
    let mut category_counts: HashMap<String, HashMap<Uuid, usize>> = HashMap::new();
    let mut arm_totals: HashMap<Uuid, usize> = HashMap::new();

    for record in records {
        *arm_totals.entry(record.arm_id).or_default() += 1;

        if let Some(values) = extract(record) {
            for value in values {
                *category_counts
                    .entry(value)
                    .or_default()
                    .entry(record.arm_id)
                    .or_default() += 1;
            }
        }
    }

    if category_counts.is_empty() {
        return None;
    }

    // Sort category values for deterministic output
    let mut category_values: Vec<String> = category_counts.keys().cloned().collect();
    category_values.sort();

    let mut categories = Vec::with_capacity(category_values.len());
    let mut max_smd: f64 = 0.0;

    for value in &category_values {
        let counts = &category_counts[value];
        let mut arms_data = Vec::with_capacity(arm_ids.len());
        let mut level_smd_stats = Vec::with_capacity(arm_ids.len());

        // Check small-cell suppression for this category value
        let needs_suppression = arm_ids.iter().any(|arm_id| {
            let count = counts.get(arm_id).copied().unwrap_or(0);
            count > 0 && count < SMALL_CELL_THRESHOLD
        });

        for &arm_id in arm_ids {
            let count = counts.get(&arm_id).copied().unwrap_or(0);
            let total = arm_totals.get(&arm_id).copied().unwrap_or(0);
            let proportion = if total > 0 {
                as_f64(count) / as_f64(total)
            } else {
                0.0
            };

            level_smd_stats.push(ArmStats {
                value: proportion,
                sd: 0.0,
            });

            if needs_suppression {
                arms_data.push(CategoricalArmData {
                    arm_id,
                    count: serde_json::Value::String("<5".to_owned()),
                    pct: serde_json::Value::String("<5".to_owned()),
                });
            } else {
                arms_data.push(CategoricalArmData {
                    arm_id,
                    count: serde_json::json!(count),
                    pct: serde_json::json!(round_pct(proportion * 100.0)),
                });
            }
        }

        // Compute SMD for this level even when suppressed (SMD itself is not PII)
        let level_smd = smd_max_pairwise(&level_smd_stats, false);
        max_smd = max_smd.max(level_smd);

        categories.push(CategoryData {
            value: value.clone(),
            arms: arms_data,
        });
    }

    Some(BalanceVariable::Categorical {
        name: name.to_owned(),
        categories,
        smd: max_smd,
    })
}

// -- Numeric helpers --

/// Converts a count to `f64`.
#[expect(
    clippy::cast_precision_loss,
    reason = "patient counts stay far below 2^52"
)]
fn as_f64(count: usize) -> f64 {
    count as f64
}

fn compute_mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / as_f64(values.len())
}

fn compute_sd(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = compute_mean(values);
    let variance =
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / as_f64(values.len() - 1);
    variance.sqrt()
}

/// Rounds a percentage to one decimal place.
fn round_pct(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // Core's `PatientDemographicsWithArm` is `#[non_exhaustive]`; build fixtures
    // through its `Deserialize` impl rather than a struct literal.
    fn make_record(
        arm_id: Uuid,
        birth_date: Option<NaiveDate>,
        sex: Option<&str>,
        bmi: Option<f64>,
    ) -> PatientDemographicsWithArm {
        serde_json::from_value(serde_json::json!({
            "patient_id": Uuid::now_v7().to_string(),
            "birth_date": birth_date,
            "sex": sex,
            "races": null,
            "ethnicity": null,
            "primary_language": null,
            "smoking_status": null,
            "insurance_type": null,
            "bmi": bmi,
            "synced_at": Utc::now(),
            "arm_id": arm_id,
        }))
        .expect("valid demographics fixture")
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 29).expect("valid date")
    }

    /// Two arms with every sex count at or above the small-cell threshold.
    fn sex_fixture(arm_a: Uuid, arm_b: Uuid) -> Vec<PatientDemographicsWithArm> {
        let mut records = Vec::new();
        records.extend((0..6).map(|_| make_record(arm_a, None, Some("Female"), None)));
        records.extend((0..4).map(|_| make_record(arm_a, None, Some("Male"), None)));
        records.extend((0..8).map(|_| make_record(arm_b, None, Some("Female"), None)));
        records.extend((0..2).map(|_| make_record(arm_b, None, Some("Male"), None)));
        records
    }

    fn continuous<'a>(response: &'a BalanceResponse, variable: &str) -> &'a [ContinuousArmData] {
        response
            .variables
            .iter()
            .find_map(|v| match v {
                BalanceVariable::Continuous { name, arms, .. } if name == variable => {
                    Some(arms.as_slice())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("continuous variable {variable} should exist"))
    }

    fn categorical<'a>(response: &'a BalanceResponse, variable: &str) -> (&'a [CategoryData], f64) {
        response
            .variables
            .iter()
            .find_map(|v| match v {
                BalanceVariable::Categorical {
                    name,
                    categories,
                    smd,
                } if name == variable => Some((categories.as_slice(), *smd)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("categorical variable {variable} should exist"))
    }

    fn arm_data(level: &CategoryData, arm_id: Uuid) -> &CategoricalArmData {
        level
            .arms
            .iter()
            .find(|a| a.arm_id == arm_id)
            .expect("arm present in every category")
    }

    #[test]
    fn empty_records_returns_empty_variables() {
        let study_id = Uuid::now_v7();
        let response = build_balance_response(study_id, &[], today());

        assert_eq!(response.total_patients, 0);
        assert!(response.arms.is_empty());
        assert!(response.variables.is_empty());
    }

    #[test]
    fn continuous_variable_computes_mean_per_arm() {
        let arm_a = Uuid::now_v7();
        let arm_b = Uuid::now_v7();
        let records = vec![
            make_record(arm_a, None, None, Some(25.0)),
            make_record(arm_a, None, None, Some(27.0)),
            make_record(arm_b, None, None, Some(30.0)),
            make_record(arm_b, None, None, Some(32.0)),
        ];

        let response = build_balance_response(Uuid::now_v7(), &records, today());

        let bmi = continuous(&response, "bmi");
        let mean_a = bmi.iter().find(|a| a.arm_id == arm_a).expect("arm a").mean;
        let mean_b = bmi.iter().find(|a| a.arm_id == arm_b).expect("arm b").mean;
        assert!((mean_a - 26.0).abs() < 1e-10);
        assert!((mean_b - 31.0).abs() < 1e-10);
    }

    #[test]
    fn age_derived_from_birth_date() {
        let birth_date = NaiveDate::from_ymd_opt(1990, 1, 1).expect("valid date");
        let records = vec![make_record(Uuid::now_v7(), Some(birth_date), None, None)];

        let response = build_balance_response(Uuid::now_v7(), &records, today());

        // Born 1990-01-01, today is 2026-06-29 => 36 years
        assert!((continuous(&response, "age")[0].mean - 36.0).abs() < 1e-10);
    }

    #[test]
    fn categorical_variable_reports_counts_and_percentages() {
        let arm_a = Uuid::now_v7();
        let arm_b = Uuid::now_v7();
        let records = sex_fixture(arm_a, arm_b);

        let response = build_balance_response(Uuid::now_v7(), &records, today());

        let (levels, _) = categorical(&response, "sex");
        let female = levels.iter().find(|c| c.value == "Female").expect("Female");
        assert_eq!(arm_data(female, arm_a).count, serde_json::json!(6));
        assert_eq!(arm_data(female, arm_a).pct, serde_json::json!(60.0));
        assert_eq!(arm_data(female, arm_b).count, serde_json::json!(8));
        assert_eq!(arm_data(female, arm_b).pct, serde_json::json!(80.0));
    }

    #[test]
    fn categorical_smd_is_positive_for_imbalanced_arms() {
        let arm_a = Uuid::now_v7();
        let arm_b = Uuid::now_v7();
        let records = sex_fixture(arm_a, arm_b);

        let response = build_balance_response(Uuid::now_v7(), &records, today());

        let (_, smd) = categorical(&response, "sex");
        assert!(smd > 0.0);
    }

    #[test]
    fn small_cell_suppression_redacts_counts_below_threshold() {
        let arm_a = Uuid::now_v7();
        let arm_b = Uuid::now_v7();
        let mut records: Vec<PatientDemographicsWithArm> = (0..3)
            .map(|_| make_record(arm_a, None, Some("Female"), None))
            .collect();
        records.extend((0..10).map(|_| make_record(arm_b, None, Some("Female"), None)));

        let response = build_balance_response(Uuid::now_v7(), &records, today());

        // arm_a is below the threshold, so every arm's cell is redacted.
        let (levels, _) = categorical(&response, "sex");
        let female = levels.iter().find(|c| c.value == "Female").expect("Female");
        for arm in &female.arms {
            assert_eq!(arm.count, serde_json::json!("<5"));
            assert_eq!(arm.pct, serde_json::json!("<5"));
        }
    }

    #[test]
    fn missing_continuous_values_tracked() {
        let arm_a = Uuid::now_v7();
        let records = vec![
            make_record(arm_a, None, None, Some(25.0)),
            make_record(arm_a, None, None, None),
            make_record(arm_a, None, None, Some(27.0)),
        ];

        let response = build_balance_response(Uuid::now_v7(), &records, today());

        assert_eq!(continuous(&response, "bmi")[0].missing, 1);
    }

    #[test]
    fn compute_mean_and_sd_basic() {
        let values = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let mean = compute_mean(&values);
        let sd = compute_sd(&values);

        assert!((mean - 5.0).abs() < 1e-10);
        // Sample SD (Bessel-corrected): variance = 32/7, SD = sqrt(32/7)
        let expected_sd = (32.0_f64 / 7.0).sqrt();
        assert!((sd - expected_sd).abs() < 1e-10);
    }

    #[test]
    fn round_pct_rounds_to_one_decimal() {
        assert!((round_pct(57.14) - 57.1).abs() < 1e-10);
        assert!((round_pct(57.15) - 57.2).abs() < 1e-10);
        assert!((round_pct(100.0) - 100.0).abs() < 1e-10);
    }
}
