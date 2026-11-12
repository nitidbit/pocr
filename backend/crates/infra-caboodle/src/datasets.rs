//! Execute only approved, predeployed procedures with bound context JSON.
//!
//! No SQL text is taken from definitions: the only statement ever executed is
//! `EXEC [schema].[name] @context_json = @P1`, built from the procedure's
//! validated identifiers, and the context travels as a bound parameter.

use std::time::Duration;

use futures::TryStreamExt;
use serde_json::Value;
use tiberius::{Column, ColumnData, Query, QueryItem};
use tracing::instrument;

use pocr_core::dataset::{
    ContextRecord, DatasetDefinition, DatasetRecord, Procedure, ResultRow, ResultsError,
};

use crate::{CaboodleClient, CaboodleError, CaboodleResult, error::driver_error};

/// Run a dataset's stored procedure for `input` and parse the result.
///
/// Binds `input` as one JSON array to the procedure's `@context_json`
/// parameter and requires exactly one result set that matches `definition`.
/// The call is abandoned once `timeout` elapses. After any error the connection
/// state is unknown, so the caller must drop `client` rather than reuse it.
///
/// # Errors
///
/// Returns [`CaboodleError::Timeout`] if the time limit passes,
/// [`CaboodleError::Query`] if the driver rejects the query or stream,
/// [`CaboodleError::ResultSetCount`] if the procedure returns zero or several
/// result sets, [`CaboodleError::Contract`] if the result set or rows violate
/// the dataset contract (columns, row count, IDs, or value types), and
/// [`CaboodleError::Conversion`] if a value is nonfinite or has an unsupported
/// SQL type.
#[instrument(
    skip_all,
    fields(dataset_name = %definition.name(), batch_size = input.len()),
    err(Display)
)]
pub async fn execute_dataset(
    client: &mut CaboodleClient,
    definition: &DatasetDefinition,
    input: &[ContextRecord],
    timeout: Duration,
) -> CaboodleResult<Vec<DatasetRecord>> {
    tokio::time::timeout(timeout, run_procedure(client, definition, input))
        .await
        .map_err(|_| CaboodleError::Timeout)?
}

async fn run_procedure(
    client: &mut CaboodleClient,
    definition: &DatasetDefinition,
    input: &[ContextRecord],
) -> CaboodleResult<Vec<DatasetRecord>> {
    let payload = serde_json::to_string(input)
        .expect("context records of UUIDs, timestamps, and JSON values encode");
    let mut query = Query::new(statement(definition.procedure()));
    // Tiberius encodes long strings as nvarchar(max); the SP parameter is always nvarchar(max).
    query.bind(payload);
    let mut stream = query
        .query(&mut client.client)
        .await
        .map_err(|e| CaboodleError::Query(driver_error(&e)))?;
    let mut result_sets = 0;
    let mut rows = Vec::new();
    while let Some(item) = stream
        .try_next()
        .await
        .map_err(|e| CaboodleError::Query(driver_error(&e)))?
    {
        match item {
            QueryItem::Metadata(metadata) => {
                result_sets += 1;
                if result_sets != 1 {
                    return Err(CaboodleError::ResultSetCount {
                        actual: result_sets,
                    });
                }
                definition.check_columns(metadata.columns().iter().map(Column::name))?;
            }
            QueryItem::Row(row) => {
                if rows.len() >= input.len() {
                    return Err(ResultsError::RowCount {
                        expected: input.len(),
                        actual: rows.len() + 1,
                    }
                    .into());
                }
                let names: Vec<_> = row.columns().iter().map(|c| c.name().to_owned()).collect();
                let values: CaboodleResult<ResultRow> = names
                    .into_iter()
                    .zip(row)
                    .map(|(name, value)| Ok((name, convert(value)?)))
                    .collect();
                rows.push(values?);
            }
        }
    }
    if result_sets != 1 {
        return Err(CaboodleError::ResultSetCount {
            actual: result_sets,
        });
    }
    Ok(definition.parse_results(input, rows)?)
}

/// Builds the statement that runs `procedure`, binding `@P1` to the context JSON.
fn statement(procedure: &Procedure) -> String {
    format!(
        "EXEC [{}].[{}] @context_json = @P1;",
        procedure.schema(),
        procedure.name()
    )
}

fn convert(value: ColumnData<'_>) -> CaboodleResult<Value> {
    fn number(v: f64) -> CaboodleResult<Value> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| CaboodleError::Conversion("nonfinite dataset number".into()))
    }
    Ok(match value {
        ColumnData::U8(v) => v.map_or(Value::Null, Value::from),
        ColumnData::I16(v) => v.map_or(Value::Null, Value::from),
        ColumnData::I32(v) => v.map_or(Value::Null, Value::from),
        ColumnData::I64(v) => v.map_or(Value::Null, Value::from),
        ColumnData::F32(v) => v
            .map(|v| number(f64::from(v)))
            .transpose()?
            .unwrap_or(Value::Null),
        ColumnData::F64(v) => v.map(number).transpose()?.unwrap_or(Value::Null),
        ColumnData::Numeric(v) => v
            .map(|v| number(f64::from(v)))
            .transpose()?
            .unwrap_or(Value::Null),
        ColumnData::Bit(v) => v.map_or(Value::Null, Value::Bool),
        ColumnData::String(v) => v.map_or(Value::Null, |s| Value::String(s.into_owned())),
        ColumnData::Guid(v) => v.map_or(Value::Null, |v| Value::String(v.to_string())),
        _ => {
            return Err(CaboodleError::Conversion(
                "unsupported dataset SQL type".into(),
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_binds_context_to_quoted_procedure() {
        let definition: DatasetDefinition = serde_json::from_value(serde_json::json!({
            "version": 1, "name": "outcomes", "title": "Outcomes", "source": "caboodle",
            "stored_procedure": {"schema": "pocr", "name": "Outcomes"},
            "fields": [{"name": "los", "type": "number", "column": "LOS"}]
        }))
        .expect("valid dataset");

        let sql = statement(definition.procedure());

        assert_eq!(sql, "EXEC [pocr].[Outcomes] @context_json = @P1;");
    }

    #[test]
    fn rejects_nonfinite_and_unsupported_values() {
        convert(ColumnData::F64(Some(f64::NAN))).unwrap_err();
        convert(ColumnData::Binary(None)).unwrap_err();
    }

    #[test]
    fn preserves_nulls_and_numeric_values() {
        assert_eq!(convert(ColumnData::Numeric(None)).unwrap(), Value::Null);
        assert_eq!(convert(ColumnData::I32(Some(42))).unwrap(), Value::from(42));
    }
}
