//! Study aggregate — the root entity configuring a randomization trial.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::environment::Environment;
use crate::random::{Arm, KeyVariant, Stratum};

/// The lifecycle state of a study.
///
/// Transitions are constrained by [`Study::transition_to`]. Once a study
/// leaves [`Draft`](StudyStatus::Draft) its allocation configuration is
/// frozen (see [`Study::config_is_frozen`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum StudyStatus {
    /// Being configured. Editable. Non-production randomization allowed for
    /// validation; production randomization is not.
    Draft,
    /// Live. Accepts production and non-production randomization requests.
    /// Allocation config is frozen.
    Active,
    /// Temporarily halted. Production randomization is rejected; the study
    /// can be returned to [`Active`](StudyStatus::Active). Non-production
    /// randomization remains allowed. Config stays frozen.
    Paused,
    /// Terminal. No new randomizations in any environment.
    Closed,
}

impl StudyStatus {
    /// The lowercase string form used in stored records.
    pub fn as_str(self) -> &'static str {
        match self {
            StudyStatus::Draft => "draft",
            StudyStatus::Active => "active",
            StudyStatus::Paused => "paused",
            StudyStatus::Closed => "closed",
        }
    }
}

/// The decision produced by the environment × status randomization gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    /// The request may proceed to randomization.
    Allow,
    /// The request is rejected; `reason` explains why (surfaced to the caller).
    Reject(GateRejection),
}

/// Why a randomization request was gated off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GateRejection {
    /// The study is not active and the environment requires an active study.
    StudyNotActive,
    /// The study is closed; no environment may randomize.
    StudyClosed,
}

/// Decides whether a request in `env` may randomize against a study in `status`.
///
/// Rules (see design spec, section 4):
/// - Production randomizes **only** when the study is `Active`.
/// - Non-production randomizes in any non-terminal state (`Draft`/`Active`/`Paused`).
/// - `Closed` rejects every environment.
///
/// Pure and total — the single source of truth for the gate, exercised
/// exhaustively by the service and by tests.
pub fn gate(env: Environment, status: StudyStatus) -> GateDecision {
    match (env, status) {
        (_, StudyStatus::Closed) => GateDecision::Reject(GateRejection::StudyClosed),
        (Environment::Production, StudyStatus::Active)
        | (
            Environment::NonProduction,
            StudyStatus::Draft | StudyStatus::Active | StudyStatus::Paused,
        ) => GateDecision::Allow,
        (Environment::Production, _) => GateDecision::Reject(GateRejection::StudyNotActive),
    }
}

/// Errors from study lifecycle operations.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum StudyError {
    /// The requested status transition is not permitted from the current state.
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// Current status.
        from: StudyStatus,
        /// Requested status.
        to: StudyStatus,
    },
    /// Allocation configuration cannot change once the study has left `Draft`.
    #[error("allocation configuration is frozen (study is not in draft)")]
    ConfigFrozen,
    /// The study's configuration is invalid and cannot go live.
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),
}

/// The randomization strategy for a study.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RandomizationStrategy {
    /// Each subject is independently randomized with equal probability.
    Simple,
    /// Subjects are randomized in permuted blocks to maintain balance.
    Block,
}

/// Configuration for how randomization is performed in a study.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RandomizationConfig {
    /// The randomization strategy.
    pub strategy: RandomizationStrategy,
    /// Treatment arms with allocation ratios.
    pub arms: Vec<Arm>,
    /// Keys determining which request field identifies the subject.
    pub keys: Vec<KeyVariant>,
    /// Stratification factor definitions.
    pub strata: Vec<Stratum>,
    /// Block size override (only relevant for Block strategy).
    /// If `None` or `0`, a dynamic block size is chosen per-block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_size: Option<u32>,
}

impl RandomizationConfig {
    /// Validates the configuration for go-live. Returns the collected errors.
    ///
    /// Mirrors the API-layer create validation so that a study cannot be
    /// activated with a config that would never randomize correctly.
    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.arms.len() < 2 {
            errors.push("at least 2 arms are required".to_string());
        }
        for (i, arm) in self.arms.iter().enumerate() {
            if arm.ratio == 0 {
                errors.push(format!("arm[{i}].ratio must be positive"));
            }
        }
        if let Some(block_size) = self.block_size {
            let ratio_sum: u32 = self.arms.iter().map(|a| a.ratio).sum();
            if ratio_sum > 0 && block_size % ratio_sum != 0 {
                errors.push(format!(
                    "block_size ({block_size}) must be a multiple of the sum of arm ratios ({ratio_sum})"
                ));
            }
        }

        errors
    }
}

/// The study aggregate root — configures a clinical trial's randomization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Study {
    /// Unique identifier (`UUIDv7`).
    pub id: Uuid,
    /// Human-readable study name.
    pub name: String,
    /// Description of the study.
    pub description: String,
    /// Lifecycle status.
    pub status: StudyStatus,
    /// Randomization configuration.
    pub randomization_config: RandomizationConfig,
    /// When the study was created.
    pub created_at: DateTime<Utc>,
    /// When the study was last modified.
    pub updated_at: DateTime<Utc>,
}

impl Study {
    /// Creates a new draft study with the given configuration.
    pub fn new(name: String, description: String, config: RandomizationConfig) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            name,
            description,
            status: StudyStatus::Draft,
            randomization_config: config,
            created_at: now,
            updated_at: now,
        }
    }

    /// Whether the allocation configuration is frozen.
    ///
    /// Config is mutable only while the study is in [`Draft`](StudyStatus::Draft).
    /// Freezing preserves allocation integrity once subjects can be randomized.
    pub fn config_is_frozen(&self) -> bool {
        self.status != StudyStatus::Draft
    }

    /// Whether `next` is a permitted transition from the current status.
    fn transition_allowed(from: StudyStatus, next: StudyStatus) -> bool {
        use StudyStatus::{Active, Closed, Draft, Paused};
        matches!(
            (from, next),
            (Draft | Paused, Active) | (Active, Paused) | (Draft | Active | Paused, Closed)
        )
    }

    /// Attempts a lifecycle transition, enforcing the state machine.
    ///
    /// Permitted transitions (design spec section 3):
    /// `Draft→Active` (validates config, then freezes it), `Draft→Closed`,
    /// `Active→Paused`, `Active→Closed`, `Paused→Active`, `Paused→Closed`.
    /// `Closed` is terminal. Any other transition is
    /// [`StudyError::InvalidTransition`]. On success, `updated_at` is stamped.
    ///
    /// # Errors
    ///
    /// Returns [`StudyError::InvalidTransition`] if the state machine forbids
    /// the transition, or [`StudyError::InvalidConfiguration`] if going live
    /// with an invalid allocation configuration.
    pub fn transition_to(&mut self, next: StudyStatus) -> Result<(), StudyError> {
        if !Self::transition_allowed(self.status, next) {
            return Err(StudyError::InvalidTransition {
                from: self.status,
                to: next,
            });
        }

        // Going live validates the allocation config before it is frozen.
        if self.status == StudyStatus::Draft && next == StudyStatus::Active {
            let errors = self.randomization_config.validate();
            if !errors.is_empty() {
                return Err(StudyError::InvalidConfiguration(errors.join("; ")));
            }
        }

        self.status = next;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Replaces the allocation configuration, honoring the freeze rule.
    ///
    /// Allowed only while in [`Draft`](StudyStatus::Draft); otherwise returns
    /// [`StudyError::ConfigFrozen`]. Cosmetic fields (`name`, `description`)
    /// are edited directly and are never frozen.
    ///
    /// # Errors
    ///
    /// Returns [`StudyError::ConfigFrozen`] if the study has left draft.
    pub fn set_config(&mut self, config: RandomizationConfig) -> Result<(), StudyError> {
        if self.config_is_frozen() {
            return Err(StudyError::ConfigFrozen);
        }
        self.randomization_config = config;
        self.updated_at = Utc::now();
        Ok(())
    }
}
