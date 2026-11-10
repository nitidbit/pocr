use std::borrow::Borrow;
use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::record::{self, ContextRecord, DatasetRecord, ResultRow, ResultsError};
use super::report::MAX_DECIMALS;

pub(super) const ROW_KEY: &str = "randomization_id";
const RESERVED_FIELDS: [&str; 3] = [ROW_KEY, "arm_id", "arm_name"];

/// Longest SQL Server identifier: `sysname` is `nvarchar(128)`.
///
/// Dataset names share the bound so every name fits the same storage.
const MAX_IDENTIFIER_LEN: usize = 128;

/// Reasons a single dataset or report definition is rejected.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DefinitionError {
    /// The definition declares a version this build cannot read.
    #[error("unsupported definition version {found}")]
    UnsupportedVersion { found: u8 },
    /// The dataset name is not 1-128 characters of `[a-z0-9-]`.
    #[error("invalid dataset name {value:?}")]
    InvalidDatasetName { value: String },
    /// The report name is not 1-128 characters of `[a-z0-9-]`.
    #[error("invalid report name {value:?}")]
    InvalidReportName { value: String },
    /// An identifier is not a bare SQL identifier of at most 128 characters.
    #[error("invalid {role} {value:?}")]
    InvalidIdentifier { role: IdentifierRole, value: String },
    /// A dataset or report title is empty or whitespace.
    #[error("title is blank")]
    BlankTitle,
    /// The dataset declares no fields.
    #[error("dataset declares no fields")]
    NoFields,
    /// A field uses a name owned by the system columns.
    #[error("field name {name:?} is reserved")]
    ReservedField { name: String },
    /// Two fields share a name.
    #[error("duplicate field {name}")]
    DuplicateField { name: FieldName },
    /// A result column name is empty or whitespace.
    #[error("column name is blank")]
    BlankColumn,
    /// Two fields share a result column, or a field claims `randomization_id`.
    #[error("duplicate column {column}")]
    DuplicateColumn { column: ColumnName },
    /// The report declares no variables.
    #[error("report declares no variables")]
    NoVariables,
    /// Two report variables summarize the same field.
    #[error("field {field} appears in more than one report variable")]
    DuplicateReportField { field: FieldName },
    /// A report variable label is empty or whitespace.
    #[error("label for field {field} is blank")]
    BlankLabel { field: FieldName },
    /// A report variable unit is empty or whitespace.
    #[error("unit for field {field} is blank")]
    BlankUnit { field: FieldName },
    /// A categorical variable declares a unit, which applies to continuous values only.
    #[error("categorical field {field} cannot declare a unit")]
    UnitOnCategorical { field: FieldName },
    /// A category is listed twice in a variable's display order.
    #[error("duplicate category {category:?} for field {field}")]
    DuplicateCategory { field: FieldName, category: String },
    /// A continuous variable declares category ordering.
    #[error("continuous field {field} cannot declare categories")]
    CategoriesOnContinuous { field: FieldName },
    /// A categorical variable declares decimals, which apply to continuous values only.
    #[error("categorical field {field} cannot declare decimals")]
    DecimalsOnCategorical { field: FieldName },
    /// A categorical variable declares an `include_when` condition.
    #[error("categorical field {field} cannot declare a condition")]
    ConditionOnCategorical { field: FieldName },
    /// Decimal places exceed the supported maximum.
    #[error("field {field} requests {found} decimals; maximum is {max}", max = MAX_DECIMALS)]
    InvalidDecimals { field: FieldName, found: u8 },
}

/// What a [`SqlIdentifier`] names, reported in [`DefinitionError::InvalidIdentifier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierRole {
    /// Schema of the dataset's stored procedure.
    ProcedureSchema,
    /// Name of the dataset's stored procedure.
    ProcedureName,
    /// A dataset field name.
    FieldName,
}

impl fmt::Display for IdentifierRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ProcedureSchema => "procedure schema",
            Self::ProcedureName => "procedure name",
            Self::FieldName => "field name",
        })
    }
}

/// Returns whether `raw` is 1-128 characters of `[a-z0-9-]`, the rule shared
/// by dataset and report names.
pub(super) fn is_catalog_name(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= MAX_IDENTIFIER_LEN
        && raw
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

/// Dataset identifier: 1-128 characters of `[a-z0-9-]`.
///
/// Serializes as a plain string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DatasetName(String);

impl DatasetName {
    /// Parses a dataset name.
    ///
    /// # Errors
    ///
    /// Returns [`DefinitionError::InvalidDatasetName`] if `raw` is empty,
    /// longer than 128 bytes, or contains characters outside `[a-z0-9-]`.
    pub fn parse(raw: impl Into<String>) -> Result<Self, DefinitionError> {
        let raw = raw.into();
        if is_catalog_name(&raw) {
            Ok(Self(raw))
        } else {
            Err(DefinitionError::InvalidDatasetName { value: raw })
        }
    }

    /// Returns the name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for DatasetName {
    type Error = DefinitionError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(raw)
    }
}

impl From<DatasetName> for String {
    fn from(name: DatasetName) -> Self {
        name.0
    }
}

impl fmt::Display for DatasetName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Bare SQL identifier: `[A-Za-z_][A-Za-z0-9_]{0,127}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SqlIdentifier(String);

impl SqlIdentifier {
    /// Parses an identifier, naming its `role` in any error.
    ///
    /// # Errors
    ///
    /// Returns [`DefinitionError::InvalidIdentifier`] if `raw` is empty,
    /// longer than 128 bytes, starts with a digit, or contains characters
    /// outside `[A-Za-z0-9_]`.
    pub fn parse(role: IdentifierRole, raw: impl Into<String>) -> Result<Self, DefinitionError> {
        let raw = raw.into();
        let is_valid = !raw.is_empty()
            && raw.len() <= MAX_IDENTIFIER_LEN
            && raw.bytes().enumerate().all(|(i, c)| {
                c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())
            });
        if is_valid {
            Ok(Self(raw))
        } else {
            Err(DefinitionError::InvalidIdentifier { role, value: raw })
        }
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<SqlIdentifier> for String {
    fn from(identifier: SqlIdentifier) -> Self {
        identifier.0
    }
}

impl fmt::Display for SqlIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Dataset field name: a [`SqlIdentifier`] that avoids the system columns
/// `randomization_id`, `arm_id`, and `arm_name`.
///
/// Serializes as a plain string and borrows as `str`, so maps keyed by
/// field name can be read with string keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FieldName(SqlIdentifier);

impl FieldName {
    /// Parses a field name.
    ///
    /// # Errors
    ///
    /// Returns [`DefinitionError::InvalidIdentifier`] if `raw` is not a bare
    /// identifier, or [`DefinitionError::ReservedField`] if it is a system
    /// column name.
    pub fn parse(raw: impl Into<String>) -> Result<Self, DefinitionError> {
        let identifier = SqlIdentifier::parse(IdentifierRole::FieldName, raw)?;
        if RESERVED_FIELDS.contains(&identifier.as_str()) {
            return Err(DefinitionError::ReservedField {
                name: identifier.into(),
            });
        }
        Ok(Self(identifier))
    }

    /// Returns the name as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<String> for FieldName {
    type Error = DefinitionError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(raw)
    }
}

impl From<FieldName> for String {
    fn from(name: FieldName) -> Self {
        name.0.into()
    }
}

impl Borrow<str> for FieldName {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Name of a column in the procedure's result set; never blank.
///
/// Serializes as a plain string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ColumnName(String);

impl ColumnName {
    /// Parses a column name.
    ///
    /// # Errors
    ///
    /// Returns [`DefinitionError::BlankColumn`] if `raw` is empty or whitespace.
    pub fn parse(raw: impl Into<String>) -> Result<Self, DefinitionError> {
        let raw = raw.into();
        if raw.trim().is_empty() {
            return Err(DefinitionError::BlankColumn);
        }
        Ok(Self(raw))
    }

    /// Returns the name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the system column that keys every result row.
    pub(super) fn row_key() -> Self {
        Self(ROW_KEY.to_owned())
    }
}

impl TryFrom<String> for ColumnName {
    type Error = DefinitionError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(raw)
    }
}

impl From<ColumnName> for String {
    fn from(name: ColumnName) -> Self {
        name.0
    }
}

impl fmt::Display for ColumnName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Database a dataset procedure runs against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The Caboodle data warehouse.
    Caboodle,
}

/// Declared type of a dataset field.
///
/// Displays as its lowercase definition keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// Free-form text.
    String,
    /// JSON number.
    Number,
    /// JSON boolean.
    Boolean,
}

impl fmt::Display for FieldType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
        })
    }
}

/// Stored procedure that produces a dataset, named by validated identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Procedure {
    schema: SqlIdentifier,
    name: SqlIdentifier,
}

impl Procedure {
    /// Schema containing the procedure.
    pub fn schema(&self) -> &SqlIdentifier {
        &self.schema
    }

    /// Procedure name.
    pub fn name(&self) -> &SqlIdentifier {
        &self.name
    }
}

/// One clinical field in export order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    name: FieldName,
    kind: FieldType,
    column: ColumnName,
}

impl Field {
    /// Name exposed to reports and exports.
    pub fn name(&self) -> &FieldName {
        &self.name
    }

    /// Declared value type.
    pub fn kind(&self) -> FieldType {
        self.kind
    }

    /// Column in the procedure's result set that supplies the value.
    pub fn column(&self) -> &ColumnName {
        &self.column
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProcedure {
    schema: String,
    name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawField {
    name: String,
    #[serde(rename = "type")]
    kind: FieldType,
    column: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDatasetDefinition {
    version: u8,
    name: String,
    title: String,
    #[serde(default)]
    description: String,
    source: Source,
    stored_procedure: RawProcedure,
    fields: Vec<RawField>,
}

/// A validated dataset: identity, source procedure, and ordered fields.
///
/// Obtainable only by deserializing, so a held value is always valid. The
/// serialized form is the shape of `dataset.yaml`; deserializing re-validates
/// it and recomputes the fingerprint. The system row key is `randomization_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawDatasetDefinition", into = "RawDatasetDefinition")]
pub struct DatasetDefinition {
    name: DatasetName,
    title: String,
    description: String,
    source: Source,
    procedure: Procedure,
    fields: Vec<Field>,
    fingerprint: String,
}

impl TryFrom<RawDatasetDefinition> for DatasetDefinition {
    type Error = DefinitionError;

    fn try_from(raw: RawDatasetDefinition) -> Result<Self, Self::Error> {
        if raw.version != 1 {
            return Err(DefinitionError::UnsupportedVersion { found: raw.version });
        }
        let name = DatasetName::parse(raw.name)?;
        if raw.title.trim().is_empty() {
            return Err(DefinitionError::BlankTitle);
        }
        let procedure = Procedure {
            schema: SqlIdentifier::parse(
                IdentifierRole::ProcedureSchema,
                raw.stored_procedure.schema,
            )?,
            name: SqlIdentifier::parse(IdentifierRole::ProcedureName, raw.stored_procedure.name)?,
        };
        if raw.fields.is_empty() {
            return Err(DefinitionError::NoFields);
        }
        let mut definition = Self {
            name,
            title: raw.title,
            description: raw.description,
            source: raw.source,
            procedure,
            fields: parse_fields(raw.fields)?,
            fingerprint: String::new(),
        };
        let encoded = serde_json::to_vec(&RawDatasetDefinition::from(definition.clone()))
            .expect("raw dataset definition serializes to JSON");
        definition.fingerprint = format!("{:x}", Sha256::digest(encoded));
        Ok(definition)
    }
}

fn parse_fields(raw_fields: Vec<RawField>) -> Result<Vec<Field>, DefinitionError> {
    let mut names = BTreeSet::new();
    let mut columns = BTreeSet::from([ColumnName::row_key()]);
    let mut fields = Vec::with_capacity(raw_fields.len());
    for field in raw_fields {
        let name = FieldName::parse(field.name)?;
        if !names.insert(name.clone()) {
            return Err(DefinitionError::DuplicateField { name });
        }
        let column = ColumnName::parse(field.column)?;
        if !columns.insert(column.clone()) {
            return Err(DefinitionError::DuplicateColumn { column });
        }
        fields.push(Field {
            name,
            kind: field.kind,
            column,
        });
    }
    Ok(fields)
}

impl From<DatasetDefinition> for RawDatasetDefinition {
    fn from(definition: DatasetDefinition) -> Self {
        Self {
            version: 1,
            name: definition.name.into(),
            title: definition.title,
            description: definition.description,
            source: definition.source,
            stored_procedure: RawProcedure {
                schema: definition.procedure.schema.into(),
                name: definition.procedure.name.into(),
            },
            fields: definition
                .fields
                .into_iter()
                .map(|field| RawField {
                    name: field.name.into(),
                    kind: field.kind,
                    column: field.column.into(),
                })
                .collect(),
        }
    }
}

impl DatasetDefinition {
    /// Unique dataset name.
    pub fn name(&self) -> &DatasetName {
        &self.name
    }

    /// Human-readable title; never blank.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Free-form description; may be empty.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Database the procedure runs against.
    pub fn source(&self) -> Source {
        self.source
    }

    /// Stored procedure that produces the dataset.
    pub fn procedure(&self) -> &Procedure {
        &self.procedure
    }

    /// Clinical fields in export order.
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Hex SHA-256 of the canonical JSON form; changes whenever the
    /// definition changes.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Looks up a field by name.
    pub fn field(&self, name: &FieldName) -> Option<&Field> {
        self.fields.iter().find(|field| &field.name == name)
    }

    /// Compares result column names with the definition's columns plus
    /// `randomization_id`; each must appear exactly once.
    ///
    /// # Errors
    ///
    /// Returns [`ResultsError::ColumnSet`] listing missing columns and
    /// unexpected or repeated ones.
    pub fn check_columns<'a>(
        &self,
        columns: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), ResultsError> {
        record::check_columns(self, columns)
    }

    /// Checks an entire procedure result against the input cohort before it
    /// is staged.
    ///
    /// Procedures own normalization and missingness; nulls are always accepted.
    ///
    /// # Errors
    ///
    /// Returns [`ResultsError::DuplicateInput`] if `input` repeats an ID,
    /// [`ResultsError::RowCount`] if `rows` and `input` differ in length, and
    /// per row: [`ResultsError::MissingRowId`], [`ResultsError::InvalidRowId`],
    /// [`ResultsError::UnexpectedRowId`], [`ResultsError::DuplicateRowId`],
    /// [`ResultsError::ColumnSet`], or [`ResultsError::TypeMismatch`].
    pub fn parse_results(
        &self,
        input: &[ContextRecord],
        rows: Vec<ResultRow>,
    ) -> Result<Vec<DatasetRecord>, ResultsError> {
        record::parse_results(self, input, rows)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn dataset_json() -> Value {
        json!({
            "version": 1,
            "name": "outcomes",
            "title": "Outcomes",
            "description": "Study outcomes",
            "source": "caboodle",
            "stored_procedure": {"schema": "pocr", "name": "Outcomes"},
            "fields": [
                {"name": "los", "type": "number", "column": "LOS"},
                {"name": "readmitted", "type": "boolean", "column": "READMITTED"}
            ]
        })
    }

    fn dataset_error(edit: impl FnOnce(&mut Value)) -> DefinitionError {
        let mut value = dataset_json();
        edit(&mut value);
        let raw: RawDatasetDefinition = serde_json::from_value(value).expect("raw shape");
        DatasetDefinition::try_from(raw).expect_err("definition should be rejected")
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let error = dataset_error(|v| v["version"] = json!(2));

        assert!(matches!(
            error,
            DefinitionError::UnsupportedVersion { found: 2 }
        ));
    }

    #[test]
    fn invalid_dataset_name_is_rejected() {
        let error = dataset_error(|v| v["name"] = json!("Bad_Name"));

        assert!(matches!(
            error,
            DefinitionError::InvalidDatasetName { value } if value == "Bad_Name"
        ));
    }

    #[test]
    fn injected_procedure_name_is_rejected_as_invalid_identifier() {
        let error = dataset_error(|v| v["stored_procedure"]["name"] = json!("x]; DROP TABLE x"));

        assert!(matches!(
            error,
            DefinitionError::InvalidIdentifier {
                role: IdentifierRole::ProcedureName,
                ..
            }
        ));
    }

    #[test]
    fn invalid_procedure_schema_is_rejected_as_invalid_identifier() {
        let error = dataset_error(|v| v["stored_procedure"]["schema"] = json!("dbo.x"));

        assert!(matches!(
            error,
            DefinitionError::InvalidIdentifier {
                role: IdentifierRole::ProcedureSchema,
                ..
            }
        ));
    }

    #[test]
    fn invalid_field_name_is_rejected_as_invalid_identifier() {
        let error = dataset_error(|v| v["fields"][0]["name"] = json!("1st"));

        assert!(matches!(
            error,
            DefinitionError::InvalidIdentifier {
                role: IdentifierRole::FieldName,
                ..
            }
        ));
    }

    #[test]
    fn blank_dataset_title_is_rejected() {
        let error = dataset_error(|v| v["title"] = json!("  "));

        assert!(matches!(error, DefinitionError::BlankTitle));
    }

    #[test]
    fn empty_fields_are_rejected() {
        let error = dataset_error(|v| v["fields"] = json!([]));

        assert!(matches!(error, DefinitionError::NoFields));
    }

    #[test]
    fn system_column_names_are_reserved_as_field_names() {
        for reserved in ["randomization_id", "arm_id", "arm_name"] {
            let error = dataset_error(|v| v["fields"][0]["name"] = json!(reserved));

            assert!(matches!(
                error,
                DefinitionError::ReservedField { name } if name == reserved
            ));
        }
    }

    #[test]
    fn duplicate_field_name_is_rejected() {
        let error = dataset_error(|v| v["fields"][1]["name"] = json!("los"));

        assert!(matches!(
            error,
            DefinitionError::DuplicateField { name } if name.as_str() == "los"
        ));
    }

    #[test]
    fn blank_column_is_rejected() {
        let error = dataset_error(|v| v["fields"][0]["column"] = json!(" "));

        assert!(matches!(error, DefinitionError::BlankColumn));
    }

    #[test]
    fn duplicate_column_is_rejected() {
        let error = dataset_error(|v| v["fields"][1]["column"] = json!("LOS"));

        assert!(matches!(
            error,
            DefinitionError::DuplicateColumn { column } if column.as_str() == "LOS"
        ));
    }

    #[test]
    fn column_named_randomization_id_is_rejected() {
        let error = dataset_error(|v| v["fields"][0]["column"] = json!("randomization_id"));

        assert!(matches!(error, DefinitionError::DuplicateColumn { .. }));
    }
}
