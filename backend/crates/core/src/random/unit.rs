use crate::error::{RandomizationError, RandomizationResult};
use crate::random::KeyVariant;
use crate::random::RandomizationRequest;
use serde::{Deserialize, Serialize};

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
    /// Extracts a unit identifier from the request using the given key.
    ///
    /// # Errors
    ///
    /// Returns [`RandomizationError::MissingField`] if the key's corresponding
    /// request field is absent. Returns [`RandomizationError::NotImplemented`]
    /// for a [`KeyVariant::FhirPath`] key.
    pub fn try_parse(
        key: &KeyVariant,
        request: &RandomizationRequest,
    ) -> RandomizationResult<Self> {
        match key {
            KeyVariant::PatientId => {
                let id = request
                    .patient_id
                    .clone()
                    .ok_or(RandomizationError::MissingField {
                        field: "patient_id",
                    })?;
                Ok(SimpleUnit::PatientId(id))
            }
            KeyVariant::EncounterId => {
                let id = request
                    .encounter_id
                    .clone()
                    .ok_or(RandomizationError::MissingField {
                        field: "encounter_id",
                    })?;
                Ok(SimpleUnit::EncounterId(id))
            }
            KeyVariant::ProviderId => {
                let id = request
                    .user_id
                    .clone()
                    .ok_or(RandomizationError::MissingField { field: "user_id" })?;
                Ok(SimpleUnit::ProviderId(id))
            }
            KeyVariant::FhirPath(_fp) => Err(RandomizationError::NotImplemented(
                "FhirPath -> Unit".to_string(),
            )),
        }
    }
}

impl Unit {
    /// Extracts a unit from the request using one or more keys.
    ///
    /// Returns [`Unit::Simple`] for a single key, [`Unit::Composite`] for
    /// multiple keys.
    ///
    /// # Errors
    ///
    /// Returns [`RandomizationError::NotFound`] if `keys` is empty.
    /// Propagates errors from [`SimpleUnit::try_parse`] for each key.
    pub fn try_parse(
        keys: &[KeyVariant],
        request: &RandomizationRequest,
    ) -> RandomizationResult<Self> {
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
