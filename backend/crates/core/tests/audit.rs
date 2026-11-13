//! Recording audit events.

use pocr_core::audit::{AuditEvent, event_type};
use serde_json::{Value, json};
use uuid::Uuid;

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
