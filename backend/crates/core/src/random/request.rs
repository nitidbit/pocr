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
    /// Epic provider identifier.
    pub provider_id: Option<String>,
}

impl RandomizationRequest {
    /// Extract Epic-native IDs from the request context JSON.
    ///
    /// Look for `epicPatientId`, `csn`, and `epicProviderId` at the
    /// top level of the context object.
    pub fn extract_epic_ids(&self) -> EpicIds {
        let Some(ctx) = &self.context else {
            return EpicIds::default();
        };
        EpicIds {
            patient_id: ctx.get("epicPatientId").and_then(|v| v.as_str()).map(String::from),
            csn: ctx.get("csn").and_then(|v| v.as_str()).map(String::from),
            provider_id: ctx.get("epicProviderId").and_then(|v| v.as_str()).map(String::from),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_epic_ids_returns_all_fields_when_present() {
        let request = RandomizationRequest {
            locator: None,
            patient_id: None,
            encounter_id: None,
            user_id: None,
            context: Some(json!({
                "epicPatientId": "E1234",
                "csn": "CSN5678",
                "epicProviderId": "PROV99"
            })),
        };

        let ids = request.extract_epic_ids();

        assert_eq!(ids.patient_id.as_deref(), Some("E1234"));
        assert_eq!(ids.csn.as_deref(), Some("CSN5678"));
        assert_eq!(ids.provider_id.as_deref(), Some("PROV99"));
    }

    #[test]
    fn extract_epic_ids_returns_default_when_context_is_none() {
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
        assert!(ids.provider_id.is_none());
    }

    #[test]
    fn extract_epic_ids_handles_partial_context() {
        let request = RandomizationRequest {
            locator: None,
            patient_id: None,
            encounter_id: None,
            user_id: None,
            context: Some(json!({
                "epicPatientId": "E1234",
                "otherField": "ignored"
            })),
        };

        let ids = request.extract_epic_ids();

        assert_eq!(ids.patient_id.as_deref(), Some("E1234"));
        assert!(ids.csn.is_none());
        assert!(ids.provider_id.is_none());
    }

    #[test]
    fn extract_epic_ids_ignores_non_string_values() {
        let request = RandomizationRequest {
            locator: None,
            patient_id: None,
            encounter_id: None,
            user_id: None,
            context: Some(json!({
                "epicPatientId": 12345,
                "csn": null
            })),
        };

        let ids = request.extract_epic_ids();

        assert!(ids.patient_id.is_none());
        assert!(ids.csn.is_none());
    }
}
