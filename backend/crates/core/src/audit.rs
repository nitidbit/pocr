//! Audit event model — an append-only history of consequential study actions.
//!
//! Every mutation of a study (lifecycle transitions, locator assignments,
//! allocation edits, data purges, deletions) records an [`AuditEvent`]. The
//! [`AuditRepository`](crate::ports::AuditRepository) port persists them; the
//! study history view reads them back newest-first.
//!
//! `event_type` is drawn from the [`event_type`] constants so producers and
//! the UI agree on a closed vocabulary.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Canonical `event_type` values recorded on audit events.
///
/// The UI maps each to a human-readable description; keep the two in sync.
pub mod event_type {
    /// A study moved between lifecycle states. Detail carries `from`/`to`.
    pub const STUDY_TRANSITIONED: &str = "study.transitioned";
    /// An arm was bound to an Epic locator. Detail carries `armId`/`locator`.
    pub const LOCATOR_SET: &str = "study.locator_set";
    /// An arm's allocation ratio was edited. Detail carries the arm and ratios.
    pub const RATIO_CHANGED: &str = "study.ratio_changed";
    /// Non-production randomizations and blocks were purged. Detail carries counts.
    pub const NONPRODUCTION_PURGED: &str = "study.nonproduction_purged";
    /// A study was deleted.
    pub const STUDY_DELETED: &str = "study.deleted";
}

/// A single recorded action against a study.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AuditEvent {
    /// Unique, time-sortable identifier (UUIDv7).
    pub id: Uuid,
    /// Study the action was performed against.
    pub study_id: Uuid,
    /// One of the [`event_type`] constants.
    pub event_type: String,
    /// Canonical identity of who performed the action (`X-Vouch-User`).
    pub actor: String,
    /// Event-specific structured payload. Defaults to `{}` when there is none.
    pub detail: Value,
    /// When the action occurred.
    pub created_at: DateTime<Utc>,
}

impl AuditEvent {
    /// Record a new audit event, stamping a fresh UUIDv7 id and the current time.
    ///
    /// `event_type` should be one of the [`event_type`] constants. `detail`
    /// carries any event-specific fields; pass [`serde_json::Value::Null`] or an
    /// empty object when there is nothing extra to record.
    pub fn record(
        study_id: &Uuid,
        event_type: impl Into<String>,
        actor: impl Into<String>,
        detail: Value,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            study_id: *study_id,
            event_type: event_type.into(),
            actor: actor.into(),
            detail,
            created_at: Utc::now(),
        }
    }

    /// Reconstruct an event from its stored fields.
    ///
    /// For persistence adapters rehydrating a row; unlike [`AuditEvent::record`]
    /// it preserves the original `id` and `created_at` rather than stamping new
    /// ones.
    pub fn from_stored(
        id: Uuid,
        study_id: Uuid,
        event_type: String,
        actor: String,
        detail: Value,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            study_id,
            event_type,
            actor,
            detail,
            created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn record_stamps_supplied_fields() {
        let study_id = Uuid::now_v7();

        let event = AuditEvent::record(
            &study_id,
            event_type::STUDY_TRANSITIONED,
            "alice@ucsf.edu",
            json!({ "from": "draft", "to": "active" }),
        );

        assert_eq!(event.study_id, study_id);
        assert_eq!(event.event_type, "study.transitioned");
        assert_eq!(event.actor, "alice@ucsf.edu");
        assert_eq!(event.detail["to"], "active");
    }

    #[test]
    fn record_generates_time_sortable_ids() {
        let study_id = Uuid::now_v7();

        let first = AuditEvent::record(&study_id, event_type::STUDY_DELETED, "a", Value::Null);
        let second = AuditEvent::record(&study_id, event_type::STUDY_DELETED, "a", Value::Null);

        assert!(second.id > first.id);
    }
}
