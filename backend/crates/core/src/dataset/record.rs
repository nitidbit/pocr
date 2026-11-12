use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::column::SystemColumn;
use super::definition::{ColumnName, DatasetDefinition, Field, FieldName, FieldType};

/// Maximum context records in one procedure call.
pub const MAX_BATCH_RECORDS: usize = 1000;
/// Maximum encoded JSON bytes in one procedure call.
pub const MAX_BATCH_BYTES: usize = 1 << 20;

/// Encoded bytes of the `[` and `]` that enclose a batch.
const BRACKETS_LEN: usize = "[]".len();
/// Encoded bytes of the `,` between two records of a batch.
const SEPARATOR_LEN: usize = ",".len();

/// Validated dataset field values keyed by field name.
pub type Values = BTreeMap<FieldName, Value>;

/// One procedure result row keyed by result column name, before validation.
pub type ResultRow = BTreeMap<String, Value>;

/// Reasons context records cannot be split into procedure batches.
///
/// Variants carry identifiers and counts only, never context values.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BatchError {
    /// A single encoded context record exceeds [`MAX_BATCH_BYTES`].
    #[error("context for randomization {randomization_id} is {bytes} bytes, over the batch limit")]
    RecordTooLarge {
        randomization_id: Uuid,
        bytes: usize,
    },
}

/// Reasons a procedure result is rejected.
///
/// Variants carry identifiers, column names, and counts only, never clinical
/// or context values.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ResultsError {
    /// The input repeats a randomization ID.
    #[error("input repeats randomization {randomization_id}")]
    DuplicateInput { randomization_id: Uuid },
    /// The procedure returned a different number of rows than it was given.
    #[error("expected {expected} rows, found {actual}")]
    RowCount { expected: usize, actual: usize },
    /// The result columns differ from the definition's columns plus `randomization_id`.
    #[error(
        "result columns differ from the definition: missing [{}], unexpected [{}]",
        comma_separated(.missing),
        comma_separated(.unexpected)
    )]
    ColumnSet {
        missing: Vec<ColumnName>,
        unexpected: Vec<String>,
    },
    /// A row has no `randomization_id`.
    #[error("row has no randomization_id")]
    MissingRowId,
    /// A row's `randomization_id` is not a UUID string.
    #[error("row randomization_id is not a UUID")]
    InvalidRowId,
    /// A row's ID is not in the input.
    #[error("row for randomization {randomization_id} was not requested")]
    UnexpectedRowId { randomization_id: Uuid },
    /// Two rows share an ID.
    #[error("more than one row for randomization {randomization_id}")]
    DuplicateRowId { randomization_id: Uuid },
    /// A value does not match its field's declared type.
    #[error("field {field} expected a {expected} value")]
    TypeMismatch {
        field: FieldName,
        expected: FieldType,
    },
}

fn comma_separated(items: &[impl fmt::Display]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Input that a dataset procedure receives for one randomization.
///
/// Only this envelope crosses into the procedure; arm metadata stays in POCR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRecord {
    /// Randomization the procedure should describe.
    pub randomization_id: Uuid,
    /// When the randomization was issued.
    pub randomization_time: DateTime<Utc>,
    /// Randomization context captured at assignment, keyed by context name.
    pub context: BTreeMap<String, Value>,
}

/// One validated procedure result row: the randomization it answers and its
/// field values keyed by field name.
///
/// Obtainable only through [`DatasetDefinition::parse_results`].
#[derive(Debug, Clone, PartialEq)]
pub struct DatasetRecord {
    randomization_id: Uuid,
    values: Values,
}

impl DatasetRecord {
    /// Randomization this row describes.
    pub fn randomization_id(&self) -> Uuid {
        self.randomization_id
    }

    /// Field values keyed by field name; every declared field is present, nulls included.
    pub fn values(&self) -> &Values {
        &self.values
    }
}

/// One published snapshot row: a randomization, its arm, and its field values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRow {
    /// Randomization this row describes.
    pub randomization_id: Uuid,
    /// Arm the randomization was assigned to.
    pub arm_id: Uuid,
    /// Arm name stored with the assignment.
    pub arm_name: String,
    /// Field values keyed by field name; every declared field is present, nulls included.
    pub values: Values,
}

pub(super) fn check_columns<'a>(
    definition: &DatasetDefinition,
    columns: impl IntoIterator<Item = &'a str>,
) -> Result<(), ResultsError> {
    let row_key = ColumnName::row_key();
    let mut remaining: Vec<&ColumnName> = definition.fields().iter().map(Field::column).collect();
    remaining.push(&row_key);
    let mut unexpected = Vec::new();
    for column in columns {
        match remaining.iter().position(|name| name.as_ref() == column) {
            Some(index) => {
                remaining.swap_remove(index);
            }
            None => unexpected.push(column.to_owned()),
        }
    }
    if remaining.is_empty() && unexpected.is_empty() {
        return Ok(());
    }
    remaining.sort();
    Err(ResultsError::ColumnSet {
        missing: remaining.into_iter().cloned().collect(),
        unexpected,
    })
}

pub(super) fn parse_results(
    definition: &DatasetDefinition,
    input: &[ContextRecord],
    rows: Vec<ResultRow>,
) -> Result<Vec<DatasetRecord>, ResultsError> {
    let mut requested = BTreeSet::new();
    for record in input {
        if !requested.insert(record.randomization_id) {
            return Err(ResultsError::DuplicateInput {
                randomization_id: record.randomization_id,
            });
        }
    }
    if rows.len() != input.len() {
        return Err(ResultsError::RowCount {
            expected: input.len(),
            actual: rows.len(),
        });
    }
    let mut seen = BTreeSet::new();
    rows.into_iter()
        .map(|row| parse_row(definition, row, &requested, &mut seen))
        .collect()
}

fn parse_row(
    definition: &DatasetDefinition,
    mut row: ResultRow,
    requested: &BTreeSet<Uuid>,
    seen: &mut BTreeSet<Uuid>,
) -> Result<DatasetRecord, ResultsError> {
    let randomization_id = row
        .get(SystemColumn::RandomizationId.as_str())
        .ok_or(ResultsError::MissingRowId)?
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or(ResultsError::InvalidRowId)?;
    if !requested.contains(&randomization_id) {
        return Err(ResultsError::UnexpectedRowId { randomization_id });
    }
    if !seen.insert(randomization_id) {
        return Err(ResultsError::DuplicateRowId { randomization_id });
    }
    check_columns(definition, row.keys().map(String::as_str))?;
    let values = definition
        .fields()
        .iter()
        .map(|field| take_value(field, &mut row))
        .collect::<Result<_, _>>()?;
    Ok(DatasetRecord {
        randomization_id,
        values,
    })
}

/// Removes `field`'s column from a row whose column set was checked.
fn take_value(field: &Field, row: &mut ResultRow) -> Result<(FieldName, Value), ResultsError> {
    let column = field.column();
    let value = row
        .remove(column.as_ref())
        .unwrap_or_else(|| panic!("column {column} is present after the column set check"));
    let is_valid = value.is_null()
        || match field.kind() {
            FieldType::String => value.is_string(),
            FieldType::Number => value.is_number(),
            FieldType::Boolean => value.is_boolean(),
        };
    if !is_valid {
        return Err(ResultsError::TypeMismatch {
            field: field.name().clone(),
            expected: field.kind(),
        });
    }
    Ok((field.name().clone(), value))
}

/// Splits `records` into batches of at most [`MAX_BATCH_RECORDS`] records and
/// [`MAX_BATCH_BYTES`] of encoded JSON.
///
/// Byte bounds apply to the encoded UTF-8 payload, a JSON array of the
/// records, including its brackets and separators.
///
/// # Errors
///
/// Returns [`BatchError::RecordTooLarge`] if one record alone exceeds the
/// byte limit.
pub fn batches(records: &[ContextRecord]) -> Result<Vec<&[ContextRecord]>, BatchError> {
    let mut batches = Vec::new();
    let mut start = 0;
    let mut bytes = BRACKETS_LEN;
    for (i, record) in records.iter().enumerate() {
        let size = serde_json::to_vec(record)
            .expect("a context record of a UUID, a timestamp, and JSON values encodes")
            .len();
        if BRACKETS_LEN + size > MAX_BATCH_BYTES {
            return Err(BatchError::RecordTooLarge {
                randomization_id: record.randomization_id,
                bytes: size,
            });
        }
        let is_full =
            i - start == MAX_BATCH_RECORDS || bytes + SEPARATOR_LEN + size > MAX_BATCH_BYTES;
        if i > start && is_full {
            batches.push(&records[start..i]);
            start = i;
            bytes = BRACKETS_LEN;
        }
        if i > start {
            bytes += SEPARATOR_LEN;
        }
        bytes += size;
    }
    if start < records.len() {
        batches.push(&records[start..]);
    }
    Ok(batches)
}
