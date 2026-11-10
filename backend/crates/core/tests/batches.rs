//! Splitting context records into bounded procedure batches.

use pocr_core::dataset::{BatchError, ContextRecord, MAX_BATCH_BYTES, MAX_BATCH_RECORDS, batches};
use serde_json::Value;
use uuid::Uuid;

const ID: Uuid = Uuid::from_u128(1);

fn record_with(key: &str, text: String) -> ContextRecord {
    ContextRecord {
        randomization_id: ID,
        randomization_time: "2026-10-01T14:30:00Z".parse().expect("fixed timestamp"),
        context: [(key.to_owned(), Value::String(text))].into(),
    }
}

fn record() -> ContextRecord {
    record_with("note", String::new())
}

#[test]
fn batches_split_when_record_count_exceeds_limit() {
    let records = vec![record(); MAX_BATCH_RECORDS + 1];

    let chunks = batches(&records).expect("batches");

    let lengths: Vec<_> = chunks.iter().map(|c| c.len()).collect();
    assert_eq!(lengths, [MAX_BATCH_RECORDS, 1]);
}

#[test]
fn batches_split_when_bytes_exceed_limit() {
    let records = vec![record_with("blob", "x".repeat(MAX_BATCH_BYTES / 3)); 4];

    let chunks = batches(&records).expect("batches");

    let lengths: Vec<_> = chunks.iter().map(|c| c.len()).collect();
    assert_eq!(lengths, [2, 2]);
}

#[test]
fn batches_count_encoded_bytes_of_multibyte_context() {
    // Each record holds a quarter of the limit in characters but three
    // quarters in UTF-8 bytes, so two fit by characters and not by bytes.
    let text = "患".repeat(MAX_BATCH_BYTES / 4);
    let records = vec![record_with("unicode", text); 2];

    let chunks = batches(&records).expect("batches");

    let lengths: Vec<_> = chunks.iter().map(|c| c.len()).collect();
    assert_eq!(lengths, [1, 1]);
}

#[test]
fn batches_reject_single_oversized_record() {
    let records = [record_with("huge", "x".repeat(MAX_BATCH_BYTES))];

    let result = batches(&records);

    assert!(matches!(
        result,
        Err(BatchError::RecordTooLarge { randomization_id, .. }) if randomization_id == ID
    ));
}
