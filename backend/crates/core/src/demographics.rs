//! Patient demographics read model for balance assessment.
//!
//! These types back the balance endpoint, which reports demographic parity
//! across a study's treatment arms. Demographics are synced from an external
//! source (Caboodle) and joined with randomization arm assignments; the
//! [`DemographicsRepository`](crate::ports::DemographicsRepository) port
//! defines how they are retrieved.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A patient's demographic record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PatientDemographics {
    /// Epic patient identifier (MRN or internal ID).
    pub patient_id: String,
    /// Date of birth, used to derive age.
    pub birth_date: Option<NaiveDate>,
    /// Administrative sex.
    pub sex: Option<String>,
    /// Reported races (multi-valued).
    pub races: Option<Vec<String>>,
    /// Reported ethnicity.
    pub ethnicity: Option<String>,
    /// Primary spoken language.
    pub primary_language: Option<String>,
    /// Smoking status.
    pub smoking_status: Option<String>,
    /// Insurance/payor type.
    pub insurance_type: Option<String>,
    /// Body mass index.
    pub bmi: Option<f64>,
    /// When this record was last synced from the source.
    pub synced_at: DateTime<Utc>,
}

/// A patient's demographics joined with their randomization arm assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PatientDemographicsWithArm {
    /// Epic patient identifier (MRN or internal ID).
    pub patient_id: String,
    /// Date of birth, used to derive age.
    pub birth_date: Option<NaiveDate>,
    /// Administrative sex.
    pub sex: Option<String>,
    /// Reported races (multi-valued).
    pub races: Option<Vec<String>>,
    /// Reported ethnicity.
    pub ethnicity: Option<String>,
    /// Primary spoken language.
    pub primary_language: Option<String>,
    /// Smoking status.
    pub smoking_status: Option<String>,
    /// Insurance/payor type.
    pub insurance_type: Option<String>,
    /// Body mass index.
    pub bmi: Option<f64>,
    /// When this record was last synced from the source.
    pub synced_at: DateTime<Utc>,
    /// Arm the patient was randomized into.
    pub arm_id: Uuid,
}
