use serde::{Deserialize, Serialize};
use crate::error::{RandomizationError, RandomizationResult};
use crate::random::{Key, KeyVariant};
use crate::random::RandomizationRequest;

/// A name-value pair extracted from a FHIR resource path.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FhirPathValue {
    /// Identifier of the FHIR path definition that produced this value.
    name: String,
    /// Resolved value at the path.
    value: String,
}

/// A single resolved randomization unit identifier.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum SimpleUnit {
    /// Identified by patient ID.
    PatientId(String),
    /// Identified by encounter ID.
    EncounterId(String),
    /// Identified by provider/user ID.
    ProviderId(String),
    /// Identified by a FHIR path extraction.
    FhirPath(FhirPathValue),
}

/// The randomization unit — one or more identifiers that together
/// define the subject being randomized.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "kind", content = "data")]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    /// A single identifier.
    Simple(SimpleUnit),
    /// Multiple identifiers combined as a composite key.
    Composite(Vec<SimpleUnit>),
}

impl SimpleUnit {
    /// Extract a unit identifier from the request using the given key.
    ///
    /// # Errors
    ///
    /// Return [`RandomizationError::MissingField`] if the key's corresponding
    /// request field is `None`. Return [`RandomizationError::NotFound`] if
    /// the key itself is `None`.
    pub fn try_parse(key: &Key, request: &RandomizationRequest) -> RandomizationResult<Self> {
        match key {
            Some(KeyVariant::PatientId) => {
                let id = request.patient_id.clone().ok_or(
                    RandomizationError::MissingField { field: "patient_id" },
                )?;
                Ok(SimpleUnit::PatientId(id))
            }
            Some(KeyVariant::EncounterId) => {
                let id = request.encounter_id.clone().ok_or(
                    RandomizationError::MissingField { field: "encounter_id" },
                )?;
                Ok(SimpleUnit::EncounterId(id))
            }
            Some(KeyVariant::ProviderId) => {
                let id = request.user_id.clone().ok_or(
                    RandomizationError::MissingField { field: "user_id" },
                )?;
                Ok(SimpleUnit::ProviderId(id))
            }
            Some(KeyVariant::FhirPath(_fp)) => Err(RandomizationError::NotImplemented(
                "FhirPath -> Unit".to_string(),
            )),
            None => Err(RandomizationError::NotFound),
        }
    }
}

impl Unit {
    /// Extract a unit from the request using one or more keys.
    ///
    /// Return [`Unit::Simple`] for a single key, [`Unit::Composite`] for
    /// multiple keys.
    ///
    /// # Errors
    ///
    /// Return [`RandomizationError::NotFound`] if `keys` is empty.
    /// Propagate errors from [`SimpleUnit::try_parse`] for each key.
    pub fn try_parse(keys: &[Key], request: &RandomizationRequest) -> RandomizationResult<Self> {
        match keys.len() {
            0 => Err(RandomizationError::NotFound),
            1 => {
                let unit = SimpleUnit::try_parse(&keys[0], request)?;
                Ok(Unit::Simple(unit))
            }
            _ => {
                let units = keys
                    .iter()
                    .map(|k| SimpleUnit::try_parse(k, request))
                    .collect::<RandomizationResult<Vec<SimpleUnit>>>()?;
                Ok(Unit::Composite(units))
            }
        }
    }
}

impl From<&SimpleUnit> for String {
    fn from(unit: &SimpleUnit) -> Self {
        match unit {
            SimpleUnit::PatientId(id) => id.clone(),
            SimpleUnit::EncounterId(id) => id.clone(),
            SimpleUnit::ProviderId(id) => id.clone(),
            SimpleUnit::FhirPath(fp) => format!("{}={}", fp.name, fp.value),
        }
    }
}

impl From<&Unit> for String {
    fn from(unit: &Unit) -> Self {
        match unit {
            Unit::Simple(su) => su.into(),
            Unit::Composite(units) => units
                .iter()
                .map(|u| u.into())
                .collect::<Vec<String>>()
                .join("|"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_request() -> RandomizationRequest {
        RandomizationRequest {
            locator: None,
            patient_id: None,
            encounter_id: None,
            user_id: None,
            context: None,
        }
    }

    #[test]
    fn try_parse_returns_error_when_patient_id_missing() {
        let key: Key = Some(KeyVariant::PatientId);
        let request = empty_request();

        let result = SimpleUnit::try_parse(&key, &request);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RandomizationError::MissingField { field: "patient_id" }
        ));
    }

    #[test]
    fn try_parse_returns_error_when_encounter_id_missing() {
        let key: Key = Some(KeyVariant::EncounterId);
        let request = empty_request();

        let result = SimpleUnit::try_parse(&key, &request);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RandomizationError::MissingField { field: "encounter_id" }
        ));
    }

    #[test]
    fn try_parse_returns_error_when_user_id_missing() {
        let key: Key = Some(KeyVariant::ProviderId);
        let request = empty_request();

        let result = SimpleUnit::try_parse(&key, &request);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RandomizationError::MissingField { field: "user_id" }
        ));
    }

    #[test]
    fn try_parse_succeeds_when_patient_id_present() {
        let key: Key = Some(KeyVariant::PatientId);
        let request = RandomizationRequest {
            patient_id: Some("P-001".to_string()),
            ..empty_request()
        };

        let result = SimpleUnit::try_parse(&key, &request);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SimpleUnit::PatientId("P-001".to_string()));
    }

    #[test]
    fn try_parse_returns_not_found_when_key_is_none() {
        let key: Key = None;
        let request = empty_request();

        let result = SimpleUnit::try_parse(&key, &request);

        assert!(matches!(
            result.unwrap_err(),
            RandomizationError::NotFound
        ));
    }

    #[test]
    fn unit_try_parse_returns_not_found_for_empty_keys() {
        let keys: &[Key] = &[];
        let request = empty_request();

        let result = Unit::try_parse(keys, &request);

        assert!(matches!(
            result.unwrap_err(),
            RandomizationError::NotFound
        ));
    }

    #[test]
    fn try_parse_returns_not_implemented_for_fhir_path() {
        use crate::random::FhirPath;

        let key: Key = Some(KeyVariant::FhirPath(FhirPath {
            id: "fp-1".to_string(),
            name: "test".to_string(),
            context: "Patient".to_string(),
            path: "/age".to_string(),
        }));
        let request = empty_request();

        let result = SimpleUnit::try_parse(&key, &request);

        assert!(matches!(
            result.unwrap_err(),
            RandomizationError::NotImplemented(_)
        ));
    }

    #[test]
    fn unit_try_parse_returns_simple_for_single_key() {
        let keys: Vec<Key> = vec![Some(KeyVariant::PatientId)];
        let request = RandomizationRequest {
            patient_id: Some("P-001".to_string()),
            ..empty_request()
        };

        let result = Unit::try_parse(&keys, &request).unwrap();

        assert!(matches!(result, Unit::Simple(SimpleUnit::PatientId(_))));
    }

    #[test]
    fn unit_try_parse_returns_composite_for_multiple_keys() {
        let keys: Vec<Key> = vec![
            Some(KeyVariant::PatientId),
            Some(KeyVariant::EncounterId),
        ];
        let request = RandomizationRequest {
            patient_id: Some("P-001".to_string()),
            encounter_id: Some("E-001".to_string()),
            ..empty_request()
        };

        let result = Unit::try_parse(&keys, &request).unwrap();

        assert!(matches!(result, Unit::Composite(ref v) if v.len() == 2));
    }
}
