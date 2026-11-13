use std::collections::BTreeMap;

/// An incoming request to randomize a subject into a study arm.
///
/// Fields are optional because different studies use different keys
/// to identify the randomization unit. The study's configured
/// [`KeyVariant`](super::KeyVariant) determines which field is required.
#[derive(Debug)]
pub struct RandomizationRequest {
    /// Caller-supplied locator for correlating the request with an
    /// external system (e.g. an Epic order ID).
    pub locator: Option<u64>,
    /// Patient identifier, required when the study key is [`KeyVariant::PatientId`](super::KeyVariant::PatientId).
    pub patient_id: Option<String>,
    /// Encounter identifier, required when the study key is [`KeyVariant::EncounterId`](super::KeyVariant::EncounterId).
    pub encounter_id: Option<String>,
    /// Provider/user identifier, required when the study key is [`KeyVariant::ProviderId`](super::KeyVariant::ProviderId).
    pub user_id: Option<String>,
    /// FHIR server namespace associated with the original request identifiers.
    pub fhir_server: Option<String>,
    /// FHIR resource context for stratification and FhirPath-based unit extraction.
    pub context: Option<serde_json::Value>,
}

/// Epic-native identifiers extracted from the CDS Hooks context.
#[derive(Debug, Clone, Default)]
pub struct EpicIds {
    /// Epic-native patient identifier (`PatientEpicId`).
    pub patient_id: Option<String>,
    /// Contact Serial Number (encounter ID in Epic).
    pub csn: Option<String>,
    /// Epic user (provider) identifier.
    pub user_id: Option<String>,
}

impl RandomizationRequest {
    /// Captures selected identifiers only; never persists prefetch resources or tokens.
    pub fn extract_context(&self) -> BTreeMap<String, serde_json::Value> {
        let epic = self.extract_epic_ids();
        [
            ("fhir_patient_id", self.patient_id.as_ref()),
            ("fhir_encounter_id", self.encounter_id.as_ref()),
            ("fhir_user_reference", self.user_id.as_ref()),
            ("fhir_server", self.fhir_server.as_ref()),
            ("epic_patient_id", epic.patient_id.as_ref()),
            ("epic_csn", epic.csn.as_ref()),
            ("epic_user_id", epic.user_id.as_ref()),
        ]
        .into_iter()
        .filter_map(|(key, value)| {
            value.map(|v| (key.to_owned(), serde_json::Value::String(v.clone())))
        })
        .collect()
    }

    /// Extracts Epic-native IDs from the FHIR resources in the request context.
    ///
    /// The context is the CDS Hooks `prefetch` block: `patient`, `encounter`,
    /// and `user` (Practitioner) FHIR resources, each present only if the EHR
    /// supplied it. Every field is best-effort — a missing resource or
    /// identifier yields `None`.
    ///
    /// - `patient_id`: the Patient's `EXTERNAL`-typed identifier value.
    /// - `csn`: the Encounter identifier whose system OID ends in `.7.3.698084.8`.
    /// - `user_id`: the user (Practitioner) `EXTERNAL`-typed identifier value.
    pub fn extract_epic_ids(&self) -> EpicIds {
        let Some(ctx) = &self.context else {
            return EpicIds::default();
        };
        EpicIds {
            patient_id: identifier_by_type(ctx.get("patient"), "EXTERNAL"),
            csn: identifier_by_system_suffix(ctx.get("encounter"), CSN_OID_SUFFIX),
            user_id: identifier_by_type(ctx.get("user"), "EXTERNAL"),
        }
    }
}

/// Stable trailing segment of the Epic CSN identifier system OID. The leading
/// segments are deployment-specific; the leading dot anchors the match to a
/// segment boundary.
const CSN_OID_SUFFIX: &str = ".7.3.698084.8";

/// Returns the value of the first identifier on `resource` whose `type.text`
/// equals `type_text`.
fn identifier_by_type(resource: Option<&serde_json::Value>, type_text: &str) -> Option<String> {
    identifiers(resource)
        .find(|id| {
            id.get("type")
                .and_then(|t| t.get("text"))
                .and_then(|t| t.as_str())
                == Some(type_text)
        })
        .and_then(|id| id.get("value").and_then(|v| v.as_str()))
        .map(String::from)
}

/// Returns the value of the first identifier on `resource` whose `system`
/// ends with `suffix`.
fn identifier_by_system_suffix(
    resource: Option<&serde_json::Value>,
    suffix: &str,
) -> Option<String> {
    identifiers(resource)
        .find(|id| {
            id.get("system")
                .and_then(|s| s.as_str())
                .is_some_and(|s| s.ends_with(suffix))
        })
        .and_then(|id| id.get("value").and_then(|v| v.as_str()))
        .map(String::from)
}

/// Iterates the `identifier` array entries of a FHIR resource, if any.
fn identifiers(resource: Option<&serde_json::Value>) -> impl Iterator<Item = &serde_json::Value> {
    resource
        .and_then(|r| r.get("identifier"))
        .and_then(|i| i.as_array())
        .into_iter()
        .flatten()
}
