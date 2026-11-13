//! Building randomization records.

use pocr_core::environment::Environment;
use pocr_core::random::{Arm, Randomization, SimpleStratum, SimpleUnit, Strata, Unit};
use uuid::Uuid;

fn sample_unit() -> Unit {
    Unit::Simple(SimpleUnit::PatientId("P-001".to_owned()))
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

    let result = Randomization::simple(&study_id, Environment::Production, &unit, &arm);

    assert_eq!(result.study_id, study_id);
    assert_eq!(result.environment, Environment::Production);
    assert!(result.strata.is_none());
    assert_eq!(result.unit, unit);
    assert_eq!(result.arm.id, arm.id);
}

#[test]
fn block_creates_randomization_with_strata() {
    let strata = Strata::Simple(SimpleStratum::FhirPath {
        name: "age_group".to_owned(),
        value: "adult".to_owned(),
    });

    let result = Randomization::block(
        &Uuid::nil(),
        Environment::Production,
        Some(&strata),
        &sample_unit(),
        &sample_arm(),
    );

    assert!(result.strata.is_some());
}

#[test]
fn block_creates_randomization_without_strata() {
    let result = Randomization::block(
        &Uuid::nil(),
        Environment::Production,
        None,
        &sample_unit(),
        &sample_arm(),
    );

    assert!(result.strata.is_none());
}
