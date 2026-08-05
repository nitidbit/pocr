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

/// Decide whether a request in `env` may randomize against a study in `status`.
///
/// Rules (see design spec, section 4):
/// - Production randomizes **only** when the study is `Active`.
/// - Non-production randomizes in any non-terminal state (`Draft`/`Active`/`Paused`).
/// - `Closed` rejects every environment.
///
/// Pure and total — the single source of truth for the gate, exercised
/// exhaustively by the service and by tests.
pub fn gate(env: Environment, status: StudyStatus) -> GateDecision {
    use Environment::*;
    use StudyStatus::*;
    match (env, status) {
        (_, Closed) => GateDecision::Reject(GateRejection::StudyClosed),
        (Production, Active) => GateDecision::Allow,
        (Production, _) => GateDecision::Reject(GateRejection::StudyNotActive),
        (NonProduction, Draft | Active | Paused) => GateDecision::Allow,
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
    /// Validate the configuration for go-live. Returns the collected errors.
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
    /// Unique identifier (UUIDv7).
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
    /// Create a new draft study with the given configuration.
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
        use StudyStatus::*;
        matches!(
            (from, next),
            (Draft, Active)
                | (Draft, Closed)
                | (Active, Paused)
                | (Active, Closed)
                | (Paused, Active)
                | (Paused, Closed)
        )
    }

    /// Attempt a lifecycle transition, enforcing the state machine.
    ///
    /// Permitted transitions (design spec section 3):
    /// `Draft→Active` (validates config, then freezes it), `Draft→Closed`,
    /// `Active→Paused`, `Active→Closed`, `Paused→Active`, `Paused→Closed`.
    /// `Closed` is terminal. Any other transition is
    /// [`StudyError::InvalidTransition`]. On success, `updated_at` is stamped.
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

    /// Replace the allocation configuration, honoring the freeze rule.
    ///
    /// Allowed only while in [`Draft`](StudyStatus::Draft); otherwise returns
    /// [`StudyError::ConfigFrozen`]. Cosmetic fields (`name`, `description`)
    /// are edited directly and are never frozen.
    pub fn set_config(&mut self, config: RandomizationConfig) -> Result<(), StudyError> {
        if self.config_is_frozen() {
            return Err(StudyError::ConfigFrozen);
        }
        self.randomization_config = config;
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft_study() -> Study {
        Study::new(
            "Test".to_string(),
            "desc".to_string(),
            RandomizationConfig {
                strategy: RandomizationStrategy::Simple,
                arms: vec![
                    Arm {
                        id: Uuid::now_v7(),
                        name: "A".into(),
                        ratio: 1,
                        locator: None,
                    },
                    Arm {
                        id: Uuid::now_v7(),
                        name: "B".into(),
                        ratio: 1,
                        locator: None,
                    },
                ],
                keys: vec![],
                strata: vec![],
                block_size: None,
            },
        )
    }

    // ── Transition matrix ────────────────────────────────────────────────

    #[test]
    fn draft_to_active_succeeds_and_freezes() {
        let mut s = draft_study();
        assert!(!s.config_is_frozen());
        s.transition_to(StudyStatus::Active).unwrap();
        assert_eq!(s.status, StudyStatus::Active);
        assert!(s.config_is_frozen());
    }

    #[test]
    fn draft_to_closed_succeeds() {
        let mut s = draft_study();
        s.transition_to(StudyStatus::Closed).unwrap();
        assert_eq!(s.status, StudyStatus::Closed);
    }

    #[test]
    fn active_pause_resume_and_close() {
        let mut s = draft_study();
        s.transition_to(StudyStatus::Active).unwrap();
        s.transition_to(StudyStatus::Paused).unwrap();
        assert_eq!(s.status, StudyStatus::Paused);
        s.transition_to(StudyStatus::Active).unwrap();
        assert_eq!(s.status, StudyStatus::Active);
        s.transition_to(StudyStatus::Closed).unwrap();
        assert_eq!(s.status, StudyStatus::Closed);
    }

    #[test]
    fn paused_to_closed_succeeds() {
        let mut s = draft_study();
        s.transition_to(StudyStatus::Active).unwrap();
        s.transition_to(StudyStatus::Paused).unwrap();
        s.transition_to(StudyStatus::Closed).unwrap();
        assert_eq!(s.status, StudyStatus::Closed);
    }

    #[test]
    fn forbidden_transitions_are_rejected() {
        // Draft → Paused
        let mut s = draft_study();
        assert!(matches!(
            s.transition_to(StudyStatus::Paused),
            Err(StudyError::InvalidTransition { .. })
        ));

        // Closed is terminal
        let mut s = draft_study();
        s.transition_to(StudyStatus::Closed).unwrap();
        for next in [StudyStatus::Draft, StudyStatus::Active, StudyStatus::Paused] {
            assert!(matches!(
                s.transition_to(next),
                Err(StudyError::InvalidTransition { .. })
            ));
        }

        // Active → Draft (no un-freeze)
        let mut s = draft_study();
        s.transition_to(StudyStatus::Active).unwrap();
        assert!(matches!(
            s.transition_to(StudyStatus::Draft),
            Err(StudyError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn go_live_rejects_invalid_config() {
        let mut s = draft_study();
        s.randomization_config.arms.truncate(1); // < 2 arms
        assert!(matches!(
            s.transition_to(StudyStatus::Active),
            Err(StudyError::InvalidConfiguration(_))
        ));
        // Status unchanged on failed go-live.
        assert_eq!(s.status, StudyStatus::Draft);
    }

    // ── Config freeze ────────────────────────────────────────────────────

    #[test]
    fn set_config_allowed_in_draft() {
        let mut s = draft_study();
        let cfg = s.randomization_config.clone();
        assert!(s.set_config(cfg).is_ok());
    }

    #[test]
    fn set_config_frozen_after_activation() {
        let mut s = draft_study();
        let cfg = s.randomization_config.clone();
        s.transition_to(StudyStatus::Active).unwrap();
        assert_eq!(s.set_config(cfg), Err(StudyError::ConfigFrozen));
    }

    // ── Env × status gate (all 8 cells) ──────────────────────────────────

    #[test]
    fn gate_matrix_is_exhaustive_and_correct() {
        use Environment::*;
        use StudyStatus::*;

        assert_eq!(
            gate(Production, Draft),
            GateDecision::Reject(GateRejection::StudyNotActive)
        );
        assert_eq!(gate(Production, Active), GateDecision::Allow);
        assert_eq!(
            gate(Production, Paused),
            GateDecision::Reject(GateRejection::StudyNotActive)
        );
        assert_eq!(
            gate(Production, Closed),
            GateDecision::Reject(GateRejection::StudyClosed)
        );

        assert_eq!(gate(NonProduction, Draft), GateDecision::Allow);
        assert_eq!(gate(NonProduction, Active), GateDecision::Allow);
        assert_eq!(gate(NonProduction, Paused), GateDecision::Allow);
        assert_eq!(
            gate(NonProduction, Closed),
            GateDecision::Reject(GateRejection::StudyClosed)
        );
    }
}
