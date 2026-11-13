use crate::environment::Environment;
use crate::random::{Arm, Strata, Unit};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// A recorded randomization assignment — the outcome of placing a
/// subject into a study arm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Randomization {
    /// Unique identifier for this assignment record.
    pub id: Uuid,
    /// Study the subject was randomized into.
    pub study_id: Uuid,
    /// Environment this assignment belongs to. Partitions production from
    /// non-production so test randomizations never mix with real trial data.
    pub environment: Environment,
    /// Stratification applied at time of assignment, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strata: Option<Strata>,
    /// The randomization unit (subject identifier).
    pub unit: Unit,
    /// The arm the subject was assigned to.
    pub arm: Arm,
    /// Timestamp when the assignment was issued.
    pub issued_at: DateTime<Utc>,
    /// Selected context captured at assignment, never refreshed by a repeat request.
    #[serde(default)]
    pub context: BTreeMap<String, serde_json::Value>,
}

impl Randomization {
    /// Creates a simple (unstratified) randomization record.
    pub fn simple(study_id: &Uuid, environment: Environment, unit: &Unit, arm: &Arm) -> Self {
        Self {
            id: Uuid::now_v7(),
            study_id: *study_id,
            environment,
            strata: None,
            unit: unit.clone(),
            arm: arm.clone(),
            issued_at: Utc::now(),
            context: BTreeMap::new(),
        }
    }

    /// Creates a block-randomization record, optionally stratified.
    pub fn block(
        study_id: &Uuid,
        environment: Environment,
        strata: Option<&Strata>,
        unit: &Unit,
        arm: &Arm,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            study_id: *study_id,
            environment,
            strata: strata.cloned(),
            unit: unit.clone(),
            arm: arm.clone(),
            issued_at: Utc::now(),
            context: BTreeMap::new(),
        }
    }
}
