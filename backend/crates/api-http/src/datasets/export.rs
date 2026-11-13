//! CSV rendering of dataset snapshots.

use std::borrow::Cow;

use csv::{QuoteStyle, Terminator, WriterBuilder};
use serde_json::Value;

use pocr_core::dataset::{Field, FieldName, Snapshot, SystemColumn};

/// Prefix `raw` with a quote when a spreadsheet would read it as a formula.
fn protect(raw: &str) -> Cow<'_, str> {
    let starts_like_formula = raw
        .trim_start_matches(|c: char| c.is_whitespace() || c.is_control())
        .starts_with(['=', '+', '-', '@']);
    if starts_like_formula {
        Cow::Owned(format!("'{raw}"))
    } else {
        Cow::Borrowed(raw)
    }
}

/// Render a field value; only strings are protected, so negative numbers stay numeric.
fn field_cell(value: Option<&Value>) -> Cow<'_, str> {
    match value {
        Some(Value::String(text)) => protect(text),
        Some(Value::Null) | None => Cow::Borrowed(""),
        Some(other) => Cow::Owned(other.to_string()),
    }
}

/// Render `snapshot` as fully quoted, CRLF-terminated CSV: the system columns,
/// then the definition's fields in declared order.
///
/// Header cells are written unprotected: system columns and field names are
/// bare SQL identifiers and cannot start like a formula.
pub(super) fn render(snapshot: &Snapshot) -> Result<Vec<u8>, csv::Error> {
    let fields: Vec<&FieldName> = snapshot
        .definition
        .fields()
        .iter()
        .map(Field::name)
        .collect();
    let mut writer = WriterBuilder::new()
        .quote_style(QuoteStyle::Always)
        .terminator(Terminator::CRLF)
        .from_writer(Vec::new());
    writer.write_record(
        SystemColumn::ALL
            .iter()
            .map(|column| column.as_str())
            .chain(fields.iter().map(AsRef::as_ref)),
    )?;
    for row in &snapshot.rows {
        let system = [
            Cow::Owned(row.randomization_id.to_string()),
            Cow::Owned(row.arm_id.to_string()),
            protect(&row.arm_name),
        ];
        let values = fields.iter().map(|name| field_cell(row.values.get(*name)));
        for cell in system.into_iter().chain(values) {
            writer.write_field(cell.as_bytes())?;
        }
        writer.write_record(None::<&[u8]>)?;
    }
    writer
        .into_inner()
        .map_err(|error| error.into_error().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protect_neutralizes_formulas_after_leading_whitespace() {
        assert_eq!(protect(" \t=1+1"), "' \t=1+1");
    }

    #[test]
    fn protect_leaves_plain_text_borrowed() {
        assert!(matches!(protect("Home"), Cow::Borrowed("Home")));
    }

    #[test]
    fn numbers_are_not_protected() {
        assert_eq!(field_cell(Some(&Value::from(-2))), "-2");
    }
}
