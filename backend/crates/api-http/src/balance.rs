//! Balance endpoint for demographic parity assessment across treatment arms.
//!
//! Computes per-variable SMD (Standardized Mean Difference) across arms and
//! applies small-cell suppression for patient privacy. Used by the frontend
//! to display a balance table for study investigators.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{NaiveDate, Utc};
use serde::Serialize;
use tracing::instrument;
use uuid::Uuid;

use infra_pg::PgDemographicsRepository;

use crate::smd::{smd_max_pairwise, ArmStats};
use crate::AppState;

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

/// Retrieve demographic balance data for a study.
///
/// Fetch demographics+arm records, aggregate per-arm stats, compute SMD for
/// each variable, and apply small-cell suppression.
///
/// Returns an empty `variables` array when no demographics have been synced.
#[instrument(name = "balance.get", skip(state), fields(%study_id))]
pub async fn get_balance(
    State(state): State<Arc<AppState>>,
    Path(study_id): Path<Uuid>,
) -> Result<Json<BalanceResponse>, StatusCode> {
    let demographics_repo = PgDemographicsRepository::new(state.pg_pool.clone());

    let records = demographics_repo
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

use infra_pg::PatientDemographicsWithArm;

/// Build the full balance response from raw demographics records.
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
    if let Some(var) = build_categorical_variable("race", records, &arm_ids, |r| {
        r.races.clone()
    }) {
        variables.push(var);
    }

    BalanceResponse {
        study_id,
        total_patients,
        arms,
        variables,
    }
}

/// Collect unique arm IDs from records, preserving insertion order.
fn collect_arm_ids(records: &[PatientDemographicsWithArm]) -> Vec<Uuid> {
    let mut seen = HashMap::new();
    let mut ids = Vec::new();
    for record in records {
        if seen.insert(record.arm_id, ()).is_none() {
            ids.push(record.arm_id);
        }
    }
    ids
}

/// Compute age in years from a birth date to a reference date.
fn age_in_years(birth_date: NaiveDate, today: NaiveDate) -> f64 {
    let years = today.years_since(birth_date).unwrap_or(0);
    f64::from(years)
}

/// Build a continuous variable (mean, SD, SMD) from an extractor function.
///
/// Return `None` if no arm has any non-missing values.
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

/// Build a categorical variable with per-level SMD and small-cell suppression.
///
/// Return `None` if no values exist across all arms.
fn build_categorical_variable(
    name: &str,
    records: &[PatientDemographicsWithArm],
    arm_ids: &[Uuid],
    extract: impl Fn(&PatientDemographicsWithArm) -> Option<Vec<String>>,
) -> Option<BalanceVariable> {
    // Collect all distinct category values and per-arm counts.
    // key: category value, value: map of arm_id -> count
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
        let needs_suppression = arm_ids
            .iter()
            .any(|arm_id| {
                let count = counts.get(arm_id).copied().unwrap_or(0);
                count > 0 && count < SMALL_CELL_THRESHOLD
            });

        for &arm_id in arm_ids {
            let count = counts.get(&arm_id).copied().unwrap_or(0);
            let total = arm_totals.get(&arm_id).copied().unwrap_or(0);
            let proportion = if total > 0 {
                count as f64 / total as f64
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

fn compute_mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn compute_sd(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = compute_mean(values);
    let variance =
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

/// Round a percentage to one decimal place.
fn round_pct(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_record(
        arm_id: Uuid,
        birth_date: Option<NaiveDate>,
        sex: Option<&str>,
        bmi: Option<f64>,
    ) -> PatientDemographicsWithArm {
        PatientDemographicsWithArm {
            patient_id: Uuid::now_v7().to_string(),
            birth_date,
            sex: sex.map(|s| s.to_owned()),
            races: None,
            ethnicity: None,
            primary_language: None,
            smoking_status: None,
            insurance_type: None,
            bmi,
            synced_at: Utc::now(),
            arm_id,
        }
    }

    const TODAY: &str = "2026-06-29";

    fn today() -> NaiveDate {
        TODAY.parse().unwrap()
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
    fn continuous_variable_computes_mean_and_sd() {
        let arm_a = Uuid::now_v7();
        let arm_b = Uuid::now_v7();

        let records = vec![
            make_record(arm_a, None, None, Some(25.0)),
            make_record(arm_a, None, None, Some(27.0)),
            make_record(arm_b, None, None, Some(30.0)),
            make_record(arm_b, None, None, Some(32.0)),
        ];

        let response = build_balance_response(Uuid::now_v7(), &records, today());

        let bmi_var = response
            .variables
            .iter()
            .find(|v| matches!(v, BalanceVariable::Continuous { name, .. } if name == "bmi"))
            .expect("bmi variable should exist");

        if let BalanceVariable::Continuous { arms, .. } = bmi_var {
            let arm_a_data = arms.iter().find(|a| a.arm_id == arm_a).unwrap();
            assert!((arm_a_data.mean - 26.0).abs() < 1e-10);

            let arm_b_data = arms.iter().find(|a| a.arm_id == arm_b).unwrap();
            assert!((arm_b_data.mean - 31.0).abs() < 1e-10);
        }
    }

    #[test]
    fn age_derived_from_birth_date() {
        let arm_a = Uuid::now_v7();
        let bd = NaiveDate::from_ymd_opt(1990, 1, 1).unwrap();

        let records = vec![make_record(arm_a, Some(bd), None, None)];

        let response = build_balance_response(Uuid::now_v7(), &records, today());

        let age_var = response
            .variables
            .iter()
            .find(|v| matches!(v, BalanceVariable::Continuous { name, .. } if name == "age"))
            .expect("age variable should exist");

        if let BalanceVariable::Continuous { arms, .. } = age_var {
            // Born 1990-01-01, today is 2026-06-29 => 36 years
            assert!((arms[0].mean - 36.0).abs() < 1e-10);
        }
    }

    #[test]
    fn categorical_variable_counts_and_proportions() {
        let arm_a = Uuid::now_v7();
        let arm_b = Uuid::now_v7();

        // All counts >= 5 to avoid small-cell suppression
        let mut records = Vec::new();
        for _ in 0..6 {
            records.push(make_record(arm_a, None, Some("Female"), None));
        }
        for _ in 0..4 {
            records.push(make_record(arm_a, None, Some("Male"), None));
        }
        for _ in 0..8 {
            records.push(make_record(arm_b, None, Some("Female"), None));
        }
        for _ in 0..2 {
            records.push(make_record(arm_b, None, Some("Male"), None));
        }

        let response = build_balance_response(Uuid::now_v7(), &records, today());

        let sex_var = response
            .variables
            .iter()
            .find(|v| matches!(v, BalanceVariable::Categorical { name, .. } if name == "sex"))
            .expect("sex variable should exist");

        if let BalanceVariable::Categorical { categories, smd, .. } = sex_var {
            let female = categories.iter().find(|c| c.value == "Female").unwrap();
            let arm_a_female = female.arms.iter().find(|a| a.arm_id == arm_a).unwrap();
            // 6 out of 10 = 60%
            assert_eq!(arm_a_female.count, serde_json::json!(6));
            assert_eq!(arm_a_female.pct, serde_json::json!(60.0));

            let arm_b_female = female.arms.iter().find(|a| a.arm_id == arm_b).unwrap();
            // 8 out of 10 = 80%
            assert_eq!(arm_b_female.count, serde_json::json!(8));
            assert_eq!(arm_b_female.pct, serde_json::json!(80.0));

            assert!(*smd > 0.0, "SMD should be non-zero for imbalanced arms");
        }
    }

    #[test]
    fn small_cell_suppression_redacts_counts_below_threshold() {
        let arm_a = Uuid::now_v7();
        let arm_b = Uuid::now_v7();

        // arm_a has 3 Female (below threshold of 5)
        let mut records: Vec<PatientDemographicsWithArm> = (0..3)
            .map(|_| make_record(arm_a, None, Some("Female"), None))
            .collect();
        // arm_b has 10 Female
        records.extend((0..10).map(|_| make_record(arm_b, None, Some("Female"), None)));

        let response = build_balance_response(Uuid::now_v7(), &records, today());

        let sex_var = response
            .variables
            .iter()
            .find(|v| matches!(v, BalanceVariable::Categorical { name, .. } if name == "sex"))
            .expect("sex variable should exist");

        if let BalanceVariable::Categorical { categories, .. } = sex_var {
            let female = categories.iter().find(|c| c.value == "Female").unwrap();
            // Both arms should be redacted since arm_a's count is below threshold
            for arm_data in &female.arms {
                assert_eq!(arm_data.count, serde_json::json!("<5"));
                assert_eq!(arm_data.pct, serde_json::json!("<5"));
            }
        }
    }

    #[test]
    fn missing_continuous_values_tracked() {
        let arm_a = Uuid::now_v7();

        let records = vec![
            make_record(arm_a, None, None, Some(25.0)),
            make_record(arm_a, None, None, None), // missing bmi
            make_record(arm_a, None, None, Some(27.0)),
        ];

        let response = build_balance_response(Uuid::now_v7(), &records, today());

        let bmi_var = response
            .variables
            .iter()
            .find(|v| matches!(v, BalanceVariable::Continuous { name, .. } if name == "bmi"))
            .expect("bmi variable should exist");

        if let BalanceVariable::Continuous { arms, .. } = bmi_var {
            assert_eq!(arms[0].missing, 1);
        }
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
