//! HL7 CDS Hooks types for EHR integration.
//!
//! Implements the CDS Hooks specification for service discovery and
//! hook invocation. These types serialize to the JSON shapes expected
//! by Epic and other CDS Hooks consumers.

use serde::{Deserialize, Serialize};

// ─── Request types (from Epic/EHR) ──────────────────────────────────────────

/// A CDS Hooks request received from the EHR.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CdsHookRequest {
    /// A unique identifier for this hook invocation.
    pub hook_instance: String,
    /// The base URL of the FHIR server associated with this request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fhir_server: Option<String>,
    /// The hook that triggered this request (e.g., "patient-view").
    pub hook: String,
    /// OAuth2 authorization for FHIR server access.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fhir_authorization: Option<FhirAuthorization>,
    /// Hook-specific contextual data.
    pub context: CdsContext,
    /// Pre-fetched FHIR resources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefetch: Option<serde_json::Value>,
    /// Vendor-specific extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<serde_json::Value>,
}

/// OAuth2 token for accessing the FHIR server.
///
/// CDS Hooks embeds the raw OAuth2 token response, whose fields are
/// snake_case (`access_token`, `token_type`, `expires_in`) — so this
/// struct deliberately omits `rename_all = "camelCase"`. All fields are
/// optional: `fhirAuthorization` may be present with a partial or empty
/// token, and a missing sub-field must not fail deserialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirAuthorization {
    /// OAuth2 bearer token for FHIR server requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    /// Token type (typically `"Bearer"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
    /// Lifetime of the token in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    /// Granted OAuth2 scopes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Client identifier the token was issued to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

/// Contextual data from the hook trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CdsContext {
    /// FHIR Patient resource ID in context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patient_id: Option<String>,
    /// FHIR Encounter resource ID in context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encounter_id: Option<String>,
    /// FHIR ID of the user (Practitioner) who triggered the hook.
    #[serde(rename = "userId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// FHIR resource references the user has selected in the EHR, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selections: Option<Vec<String>>,
}

// ─── Response types (to Epic/EHR) ───────────────────────────────────────────

/// The response returned to the EHR after a CDS Hooks invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdsHookResponse {
    /// Decision support cards to display to the clinician.
    pub cards: Vec<Card>,
}

/// A decision support card displayed to the clinician.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    /// Unique card identifier, for tracking suggestion acceptance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// One-sentence summary shown to the clinician.
    pub summary: String,
    /// Optional longer detail (Markdown).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Urgency level.
    pub indicator: Indicator,
    /// Attribution for the card content.
    pub source: Source,
    /// Suggested actions the clinician can take.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<Suggestion>>,
    /// External links surfaced on the card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<Link>>,
}

/// Card urgency indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Indicator {
    /// Informational; no action required.
    Info,
    /// Notable; clinician should review.
    Warning,
    /// Urgent; demands clinician attention.
    Critical,
}

/// Attribution for the card content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Human-readable source name.
    pub label: String,
    /// Link to the source of the guidance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// URL of a small icon representing the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Coded topic categorizing the card content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<Coding>,
}

/// A suggested action the clinician can take.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// Human-readable label for the suggestion.
    pub label: String,
    /// Unique suggestion identifier, for tracking acceptance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// Whether the EHR should present this as the recommended choice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_recommended: Option<bool>,
    /// FHIR actions applied together if the suggestion is accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<Action>>,
}

/// A FHIR action within a suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// The kind of change to apply.
    #[serde(rename = "type")]
    pub action_type: ActionType,
    /// Human-readable description of the action.
    pub description: String,
    /// FHIR resource the action operates on (required for create/update).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<serde_json::Value>,
}

/// The type of FHIR action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionType {
    /// Create the supplied resource.
    Create,
    /// Update the supplied resource.
    Update,
    /// Delete the referenced resource.
    Delete,
}

/// An external link on a card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    /// Human-readable label for the link.
    pub label: String,
    /// Link target URL.
    pub url: String,
    /// Link type, e.g. `"absolute"` or `"smart"`.
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_type: Option<String>,
}

/// A coded value (system + code + display).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coding {
    /// Code system URI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Code within the system.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Human-readable display text for the code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

// ─── Service Discovery ──────────────────────────────────────────────────────

/// Response for `GET /cds-services` discovery endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdsServiceDiscovery {
    /// The CDS services this server offers.
    pub services: Vec<CdsServiceDescriptor>,
}

/// Metadata about a single CDS service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdsServiceDescriptor {
    /// Hook this service responds to (e.g. `"patient-view"`).
    pub hook: String,
    /// Human-readable service title.
    pub title: String,
    /// Description of what the service does.
    pub description: String,
    /// Service identifier, used in the invocation URL path.
    pub id: String,
    /// FHIR resources the EHR should prefetch, keyed by template name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefetch: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An Epic-shaped `patient-view` request with a full, snake_case
    /// OAuth2 token under `fhirAuthorization`.
    fn epic_request_json() -> serde_json::Value {
        serde_json::json!({
            "hookInstance": "d1577c69-dfbe-44ad-ba6d-3e05e953b2ea",
            "fhirServer": "https://tst.epic.example/api/FHIR/R4",
            "hook": "patient-view",
            "fhirAuthorization": {
                "access_token": "eyJraWQiOiJlLXAtb2F1dGgtY2VydA",
                "token_type": "Bearer",
                "expires_in": 3600,
                "scope": "patient/*.read",
                "subject": "cds-service4"
            },
            "context": {
                "patientId": "eXbCe123",
                "encounterId": "eKq456",
                "userId": "Practitioner/ePr789"
            }
        })
    }

    #[test]
    fn deserializes_snake_case_fhir_authorization() {
        let req: CdsHookRequest = serde_json::from_value(epic_request_json()).unwrap();
        let auth = req.fhir_authorization.expect("fhirAuthorization present");
        assert_eq!(auth.access_token.as_deref(), Some("eyJraWQiOiJlLXAtb2F1dGgtY2VydA"));
        assert_eq!(auth.token_type.as_deref(), Some("Bearer"));
        assert_eq!(auth.expires_in, Some(3600));
        assert_eq!(auth.scope.as_deref(), Some("patient/*.read"));
        assert_eq!(auth.subject.as_deref(), Some("cds-service4"));
    }

    #[test]
    fn deserializes_fhir_authorization_missing_access_token() {
        let mut json = epic_request_json();
        json["fhirAuthorization"]
            .as_object_mut()
            .unwrap()
            .remove("access_token");

        let req: CdsHookRequest = serde_json::from_value(json).unwrap();
        let auth = req.fhir_authorization.expect("fhirAuthorization present");
        assert_eq!(auth.access_token, None);
        assert_eq!(auth.token_type.as_deref(), Some("Bearer"));
    }

    #[test]
    fn deserializes_request_without_fhir_authorization() {
        let mut json = epic_request_json();
        json.as_object_mut().unwrap().remove("fhirAuthorization");

        let req: CdsHookRequest = serde_json::from_value(json).unwrap();
        assert!(req.fhir_authorization.is_none());
    }

    #[test]
    fn serializes_authorization_fields_as_snake_case() {
        let req: CdsHookRequest = serde_json::from_value(epic_request_json()).unwrap();
        let value = serde_json::to_value(&req).unwrap();
        let auth = &value["fhirAuthorization"];
        assert_eq!(auth["access_token"], "eyJraWQiOiJlLXAtb2F1dGgtY2VydA");
        assert_eq!(auth["token_type"], "Bearer");
        assert_eq!(auth["expires_in"], 3600);
    }

    #[test]
    fn skips_none_authorization_fields_on_serialize() {
        let auth = FhirAuthorization {
            access_token: Some("tok".into()),
            token_type: None,
            expires_in: None,
            scope: None,
            subject: None,
        };
        let value = serde_json::to_value(&auth).unwrap();
        let obj = value.as_object().unwrap();
        assert_eq!(obj.len(), 1);
        assert_eq!(obj["access_token"], "tok");
    }
}
