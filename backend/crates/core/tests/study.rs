//! Study lifecycle, configuration freeze, and the randomization gate.

use pocr_core::environment::Environment;
use pocr_core::random::Arm;
use pocr_core::study::{
    GateDecision, GateRejection, RandomizationConfig, RandomizationStrategy, Study, StudyError,
    StudyStatus, gate,
};
use uuid::Uuid;

fn arm(name: &str) -> Arm {
    Arm {
        id: Uuid::now_v7(),
        name: name.to_owned(),
        ratio: 1,
        locator: None,
    }
}

fn draft_study() -> Study {
    Study::new(
        "Test".to_owned(),
        "desc".to_owned(),
        RandomizationConfig {
            strategy: RandomizationStrategy::Simple,
            arms: vec![arm("A"), arm("B")],
            keys: vec![],
            strata: vec![],
            block_size: None,
        },
    )
}

/// A draft study moved through `steps` in order.
fn study_after(steps: &[StudyStatus]) -> Study {
    let mut study = draft_study();
    for &step in steps {
        study.transition_to(step).expect("permitted transition");
    }
    study
}

// Transition matrix

#[test]
fn draft_to_active_freezes_config() {
    let mut study = draft_study();
    assert!(!study.config_is_frozen());

    study.transition_to(StudyStatus::Active).expect("go live");

    assert_eq!(study.status, StudyStatus::Active);
    assert!(study.config_is_frozen());
}

#[test]
fn draft_closes_directly() {
    let mut study = draft_study();

    study.transition_to(StudyStatus::Closed).expect("close");

    assert_eq!(study.status, StudyStatus::Closed);
}

#[test]
fn active_study_pauses_and_resumes() {
    let mut study = study_after(&[StudyStatus::Active]);

    study.transition_to(StudyStatus::Paused).expect("pause");
    assert_eq!(study.status, StudyStatus::Paused);

    study.transition_to(StudyStatus::Active).expect("resume");
    assert_eq!(study.status, StudyStatus::Active);
}

#[test]
fn active_study_closes() {
    let mut study = study_after(&[StudyStatus::Active]);

    study.transition_to(StudyStatus::Closed).expect("close");

    assert_eq!(study.status, StudyStatus::Closed);
}

#[test]
fn paused_study_closes() {
    let mut study = study_after(&[StudyStatus::Active, StudyStatus::Paused]);

    study.transition_to(StudyStatus::Closed).expect("close");

    assert_eq!(study.status, StudyStatus::Closed);
}

#[test]
fn draft_cannot_pause() {
    let mut study = draft_study();

    let result = study.transition_to(StudyStatus::Paused);

    assert!(matches!(result, Err(StudyError::InvalidTransition { .. })));
}

#[test]
fn closed_is_terminal() {
    let mut study = study_after(&[StudyStatus::Closed]);

    for next in [StudyStatus::Draft, StudyStatus::Active, StudyStatus::Paused] {
        assert!(
            matches!(
                study.transition_to(next),
                Err(StudyError::InvalidTransition { .. })
            ),
            "closed study moved to {next:?}"
        );
    }
}

#[test]
fn active_cannot_return_to_draft() {
    let mut study = study_after(&[StudyStatus::Active]);

    let result = study.transition_to(StudyStatus::Draft);

    assert!(matches!(result, Err(StudyError::InvalidTransition { .. })));
}

#[test]
fn go_live_rejects_invalid_config_and_keeps_status() {
    let mut study = draft_study();
    study.randomization_config.arms.truncate(1);

    let result = study.transition_to(StudyStatus::Active);

    assert!(matches!(result, Err(StudyError::InvalidConfiguration(_))));
    assert_eq!(study.status, StudyStatus::Draft);
}

// Config freeze

#[test]
fn set_config_succeeds_in_draft() {
    let mut study = draft_study();
    let config = study.randomization_config.clone();

    study.set_config(config).expect("draft config is editable");
}

#[test]
fn set_config_fails_after_activation() {
    let mut study = study_after(&[StudyStatus::Active]);
    let config = study.randomization_config.clone();

    assert_eq!(study.set_config(config), Err(StudyError::ConfigFrozen));
}

// Environment x status gate

#[test]
fn production_gate_allows_only_active_studies() {
    let reject = GateDecision::Reject(GateRejection::StudyNotActive);

    assert_eq!(gate(Environment::Production, StudyStatus::Draft), reject);
    assert_eq!(
        gate(Environment::Production, StudyStatus::Active),
        GateDecision::Allow
    );
    assert_eq!(gate(Environment::Production, StudyStatus::Paused), reject);
}

#[test]
fn nonproduction_gate_allows_every_open_study() {
    for status in [StudyStatus::Draft, StudyStatus::Active, StudyStatus::Paused] {
        assert_eq!(
            gate(Environment::NonProduction, status),
            GateDecision::Allow,
            "{status:?}"
        );
    }
}

#[test]
fn gate_rejects_closed_studies_in_every_environment() {
    for env in [Environment::Production, Environment::NonProduction] {
        assert_eq!(
            gate(env, StudyStatus::Closed),
            GateDecision::Reject(GateRejection::StudyClosed),
            "{env}"
        );
    }
}
