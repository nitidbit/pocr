//! Resolving a request's environment from its `fhirServer` URL.

use pocr_core::environment::{Environment, EnvironmentError, EnvironmentMap};

fn sample_map() -> EnvironmentMap {
    EnvironmentMap::new([
        (
            "https://prd.epic.ucsf.edu/".to_owned(),
            Environment::Production,
        ),
        (
            "https://tst.epic.ucsf.edu/".to_owned(),
            Environment::NonProduction,
        ),
    ])
}

#[test]
fn resolve_returns_production_for_production_prefix() {
    let env = sample_map()
        .resolve(Some("https://prd.epic.ucsf.edu/api/FHIR/R4/"))
        .expect("known prefix");

    assert_eq!(env, Environment::Production);
}

#[test]
fn resolve_returns_nonproduction_for_nonproduction_prefix() {
    let env = sample_map()
        .resolve(Some("https://tst.epic.ucsf.edu/api/FHIR/R4/"))
        .expect("known prefix");

    assert_eq!(env, Environment::NonProduction);
}

#[test]
fn resolve_fails_closed_when_url_missing() {
    assert_eq!(
        sample_map().resolve(None),
        Err(EnvironmentError::MissingUrl)
    );
}

#[test]
fn resolve_fails_closed_when_url_unknown() {
    let result = sample_map().resolve(Some("https://unknown.example.com/FHIR/"));

    assert!(matches!(result, Err(EnvironmentError::UnknownUrl(_))));
}

#[test]
fn resolve_rejects_everything_when_map_empty() {
    let map = EnvironmentMap::default();

    assert!(map.is_empty());
    assert!(matches!(
        map.resolve(Some("https://prd.epic.ucsf.edu/")),
        Err(EnvironmentError::UnknownUrl(_))
    ));
}

#[test]
fn resolve_prefers_longest_matching_prefix() {
    let map = EnvironmentMap::new([
        (
            "https://epic.ucsf.edu/".to_owned(),
            Environment::NonProduction,
        ),
        (
            "https://epic.ucsf.edu/prd/".to_owned(),
            Environment::Production,
        ),
    ]);

    assert_eq!(
        map.resolve(Some("https://epic.ucsf.edu/prd/FHIR/")),
        Ok(Environment::Production)
    );
    assert_eq!(
        map.resolve(Some("https://epic.ucsf.edu/tst/FHIR/")),
        Ok(Environment::NonProduction)
    );
}

#[test]
fn parse_inverts_as_str() {
    for env in [Environment::Production, Environment::NonProduction] {
        assert_eq!(Environment::parse(env.as_str()), Some(env));
    }
}

#[test]
fn as_str_is_the_stored_form() {
    assert_eq!(Environment::Production.as_str(), "production");
    assert_eq!(Environment::NonProduction.as_str(), "nonproduction");
}
