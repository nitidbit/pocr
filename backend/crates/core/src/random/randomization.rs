use crate::random::{Arm, Strata, Unit};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A recorded randomization assignment — the outcome of placing a
/// subject into a study arm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Randomization {
    /// Unique identifier for this assignment record.
    pub id: Uuid,
    /// Study the subject was randomized into.
    pub study_id: Uuid,
    /// Stratification applied at time of assignment, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strata: Option<Strata>,
    /// The randomization unit (subject identifier).
    pub unit: Unit,
    /// The arm the subject was assigned to.
    pub arm: Arm,
    /// Timestamp when the assignment was issued.
    pub issued_at: DateTime<Utc>,
    /// Epic-native patient identifier (PatientEpicId), used as join key for Caboodle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_patient_id: Option<String>,
    /// Contact Serial Number (encounter ID in Epic).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub csn: Option<String>,
    /// Epic provider identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_provider_id: Option<String>,
}

impl Randomization {
    /// Create a simple (unstratified) randomization record.
    pub fn simple(study_id: &Uuid, unit: &Unit, arm: &Arm) -> Self {
        Self {
            id: Uuid::now_v7(),
            study_id: *study_id,
            strata: None,
            unit: unit.clone(),
            arm: arm.clone(),
            issued_at: Utc::now(),
            epic_patient_id: None,
            csn: None,
            epic_provider_id: None,
        }
    }

    /// Create a block-randomization record, optionally stratified.
    pub fn block(study_id: &Uuid, strata: Option<&Strata>, unit: &Unit, arm: &Arm) -> Self {
        Self {
            id: Uuid::now_v7(),
            study_id: *study_id,
            strata: strata.cloned(),
            unit: unit.clone(),
            arm: arm.clone(),
            issued_at: Utc::now(),
            epic_patient_id: None,
            csn: None,
            epic_provider_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::{SimpleStratum, SimpleUnit};

    fn sample_unit() -> Unit {
        Unit::Simple(SimpleUnit::PatientId("P-001".to_string()))
    }

    fn sample_arm() -> Arm {
        Arm {
            id: Uuid::nil(),
            name: String::new(),
            ratio: 1,
            locator: Some(100),
        }
    }

    #[test]
    fn simple_creates_randomization_without_strata() {
        let study_id = Uuid::nil();
        let unit = sample_unit();
        let arm = sample_arm();

        let result = Randomization::simple(&study_id, &unit, &arm);

        assert_eq!(result.study_id, study_id);
        assert!(result.strata.is_none());
        assert_eq!(result.unit, unit);
        assert_eq!(result.arm.id, arm.id);
    }

    #[test]
    fn block_creates_randomization_with_strata() {
        let study_id = Uuid::nil();
        let unit = sample_unit();
        let arm = sample_arm();
        let strata = Strata::Simple(SimpleStratum::FhirPath {
            name: "age_group".to_string(),
            value: "adult".to_string(),
        });

        let result = Randomization::block(&study_id, Some(&strata), &unit, &arm);

        assert_eq!(result.study_id, study_id);
        assert!(result.strata.is_some());
    }

    #[test]
    fn block_creates_randomization_without_strata() {
        let study_id = Uuid::nil();
        let unit = sample_unit();
        let arm = sample_arm();

        let result = Randomization::block(&study_id, None, &unit, &arm);

        assert!(result.strata.is_none());
    }
}
