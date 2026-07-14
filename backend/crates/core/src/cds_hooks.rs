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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FhirAuthorization {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
    pub subject: String,
}

/// Contextual data from the hook trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CdsContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patient_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encounter_id: Option<String>,
    #[serde(rename = "userId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selections: Option<Vec<String>>,
}

// ─── Response types (to Epic/EHR) ───────────────────────────────────────────

/// The response returned to the EHR after a CDS Hooks invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdsHookResponse {
    pub cards: Vec<Card>,
}

/// A decision support card displayed to the clinician.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub indicator: Indicator,
    pub source: Source,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<Suggestion>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<Link>>,
}

/// Card urgency indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Indicator {
    Info,
    Warning,
    Critical,
}

/// Attribution for the card content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<Coding>,
}

/// A suggested action the clinician can take.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_recommended: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<Action>>,
}

/// A FHIR action within a suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    #[serde(rename = "type")]
    pub action_type: ActionType,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<serde_json::Value>,
}

/// The type of FHIR action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionType {
    Create,
    Update,
    Delete,
}

/// An external link on a card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub label: String,
    pub url: String,
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_type: Option<String>,
}

/// A coded value (system + code + display).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

// ─── Service Discovery ──────────────────────────────────────────────────────

/// Response for `GET /cds-services` discovery endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdsServiceDiscovery {
    pub services: Vec<CdsServiceDescriptor>,
}

/// Metadata about a single CDS service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdsServiceDescriptor {
    pub hook: String,
    pub title: String,
    pub description: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefetch: Option<serde_json::Value>,
}
