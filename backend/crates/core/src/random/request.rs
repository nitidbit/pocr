/// An incoming request to randomize a subject into a study arm.
///
/// Fields are optional because different studies use different keys
/// to identify the randomization unit. The study's configured
/// [`Key`](super::Key) determines which field is required.
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
    /// FHIR resource context for stratification and FhirPath-based unit extraction.
    pub context: Option<serde_json::Value>,
}

/// Epic-native identifiers extracted from the CDS Hooks context.
#[derive(Debug, Clone, Default)]
pub struct EpicIds {
    /// Epic-native patient identifier (PatientEpicId).
    pub patient_id: Option<String>,
    /// Contact Serial Number (encounter ID in Epic).
    pub csn: Option<String>,
    /// Epic user (provider) identifier.
    pub user_id: Option<String>,
}

impl RandomizationRequest {
    /// Extract Epic-native IDs from the FHIR resources in the request context.
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

/// Return the value of the first identifier on `resource` whose `type.text`
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

/// Return the value of the first identifier on `resource` whose `system`
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

/// Iterate the `identifier` array entries of a FHIR resource, if any.
fn identifiers(resource: Option<&serde_json::Value>) -> impl Iterator<Item = &serde_json::Value> {
    resource
        .and_then(|r| r.get("identifier"))
        .and_then(|i| i.as_array())
        .into_iter()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request_with(context: serde_json::Value) -> RandomizationRequest {
        RandomizationRequest {
            locator: None,
            patient_id: None,
            encounter_id: None,
            user_id: None,
            context: Some(context),
        }
    }

    #[test]
    fn extract_patient_id_reads_external_identifier() {
        let request = request_with(json!({
            "patient": {
                "resourceType": "Patient",
                "identifier": [
                    { "type": { "text": "EXTERNAL" },
                      "system": "urn:oid:1.2.840.114350.1.13.266.3.7.2.698084",
                      "value": "Z2134" }
                ]
            }
        }));

        let ids = request.extract_epic_ids();

        assert_eq!(ids.patient_id.as_deref(), Some("Z2134"));
    }

    #[test]
    fn extract_patient_id_does_not_match_internal_identifier() {
        // INTERNAL shares the EXTERNAL system but carries a padded value; only
        // the EXTERNAL-typed entry must be selected.
        let request = request_with(json!({
            "patient": {
                "identifier": [
                    { "type": { "text": "INTERNAL" },
                      "system": "urn:oid:1.2.840.114350.1.13.266.3.7.2.698084",
                      "value": "     Z2134" },
                    { "type": { "text": "EXTERNAL" },
                      "system": "urn:oid:1.2.840.114350.1.13.266.3.7.2.698084",
                      "value": "Z2134" }
                ]
            }
        }));

        let ids = request.extract_epic_ids();

        assert_eq!(ids.patient_id.as_deref(), Some("Z2134"));
    }

    #[test]
    fn extract_csn_reads_encounter_identifier_by_oid_suffix() {
        // The CSN system OID has variable segments; only the trailing
        // `.7.3.698084.8` is stable. A sibling identifier must be ignored.
        let request = request_with(json!({
            "encounter": {
                "resourceType": "Encounter",
                "identifier": [
                    { "system": "urn:oid:1.2.840.114350.1.13.266.3.7.3.698084.7",
                      "value": "OTHER" },
                    { "system": "urn:oid:1.2.840.114350.1.13.266.3.7.3.698084.8",
                      "value": "830100123" }
                ]
            }
        }));

        let ids = request.extract_epic_ids();

        assert_eq!(ids.csn.as_deref(), Some("830100123"));
    }

    #[test]
    fn extract_user_id_reads_practitioner_external_identifier() {
        let request = request_with(json!({
            "user": {
                "resourceType": "Practitioner",
                "identifier": [
                    { "type": { "text": "EXTERNAL" },
                      "system": "urn:oid:1.2.840.114350.1.13.266.3.7.2.697780",
                      "value": "1234" }
                ]
            }
        }));

        let ids = request.extract_epic_ids();

        assert_eq!(ids.user_id.as_deref(), Some("1234"));
    }

    #[test]
    fn extract_patient_only_prefetch_leaves_csn_and_provider_none() {
        let request = request_with(json!({
            "patient": {
                "identifier": [
                    { "type": { "text": "EXTERNAL" }, "value": "Z2134" }
                ]
            }
        }));

        let ids = request.extract_epic_ids();

        assert_eq!(ids.patient_id.as_deref(), Some("Z2134"));
        assert!(ids.csn.is_none());
        assert!(ids.user_id.is_none());
    }

    #[test]
    fn extract_returns_all_none_when_context_absent() {
        let request = RandomizationRequest {
            locator: None,
            patient_id: None,
            encounter_id: None,
            user_id: None,
            context: None,
        };

        let ids = request.extract_epic_ids();

        assert!(ids.patient_id.is_none());
        assert!(ids.csn.is_none());
        assert!(ids.user_id.is_none());
    }
}
