use serde::{Deserialize, Serialize};

/// The field used to identify a randomization subject on a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum KeyVariant {
    /// Use the patient identifier.
    PatientId,
    /// Use the encounter identifier.
    EncounterId,
    /// Use the provider (user) identifier.
    ProviderId,
    /// Use a value extracted from a FHIR resource path.
    FhirPath(FhirPath),
}

/// A pointer into a FHIR resource for extracting a stratification or
/// unit value from the request context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FhirPath {
    /// Unique identifier for this path definition.
    pub id: String,
    /// Human-readable name for display or logging.
    pub name: String,
    /// FHIR resource type to look up in the request context (e.g. `"Patient"`).
    pub context: String,
    /// JSON pointer path within the resource (e.g. `"/gender"`).
    pub path: String,
}
