use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::random::Arm;

use super::definition::{
    DatasetDefinition, DatasetName, DefinitionError, FieldName, FieldType, is_catalog_name,
};
use super::record::DatasetRow;
use super::summary::{self, Report};

/// Most decimal places a continuous variable may display.
///
/// `report.schema.json` enforces the same bound; raising it means updating
/// the schema and the report contract in the datasets README.
pub(super) const MAX_DECIMALS: u8 = 6;

/// Report identifier: 1-128 characters of `[a-z0-9-]`.
///
/// Report names and dataset names are separate namespaces. Serializes as a
/// plain string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ReportName(String);

impl ReportName {
    /// Parses a report name.
    ///
    /// # Errors
    ///
    /// Returns [`DefinitionError::InvalidReportName`] if `raw` is empty,
    /// longer than 128 bytes, or contains characters outside `[a-z0-9-]`.
    pub fn parse(raw: impl Into<String>) -> Result<Self, DefinitionError> {
        let raw = raw.into();
        if is_catalog_name(&raw) {
            Ok(Self(raw))
        } else {
            Err(DefinitionError::InvalidReportName { value: raw })
        }
    }

    /// Returns the name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ReportName {
    type Error = DefinitionError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(raw)
    }
}

impl From<ReportName> for String {
    fn from(name: ReportName) -> Self {
        name.0
    }
}

impl fmt::Display for ReportName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReport {
    version: u8,
    name: String,
    dataset: String,
    title: String,
    description: String,
    variables: Vec<Variable>,
}

/// A report as deserialized from `report.yaml`.
///
/// Obtainable only by deserializing, which runs every check that does not
/// need the dataset, so a held value is intrinsically valid. Call
/// [`bind`](Self::bind) to check it against the dataset it names.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "RawReport")]
pub struct ReportDefinition {
    name: ReportName,
    dataset: DatasetName,
    title: String,
    description: String,
    variables: Vec<Variable>,
}

impl TryFrom<RawReport> for ReportDefinition {
    type Error = DefinitionError;

    fn try_from(raw: RawReport) -> Result<Self, Self::Error> {
        if raw.version != 1 {
            return Err(DefinitionError::UnsupportedVersion { found: raw.version });
        }
        let name = ReportName::parse(raw.name)?;
        let dataset = DatasetName::parse(raw.dataset)?;
        if raw.title.trim().is_empty() {
            return Err(DefinitionError::BlankTitle);
        }
        if raw.variables.is_empty() {
            return Err(DefinitionError::NoVariables);
        }
        let mut seen = BTreeSet::new();
        for variable in &raw.variables {
            variable.check()?;
            if !seen.insert(&variable.field) {
                return Err(DefinitionError::DuplicateReportField {
                    field: variable.field.clone(),
                });
            }
        }
        Ok(Self {
            name,
            dataset,
            title: raw.title,
            description: raw.description,
            variables: raw.variables,
        })
    }
}

/// One summarized dataset field.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub(super) field: FieldName,
    pub(super) label: String,
    pub(super) analysis: Analysis,
    pub(super) unit: Option<String>,
    pub(super) decimals: Option<u8>,
    pub(super) note: Option<String>,
    #[serde(default)]
    pub(super) category_order: Vec<String>,
    pub(super) include_when: Option<Condition>,
}

/// How a variable is summarized.
///
/// Displays as its lowercase definition keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Analysis {
    /// Counts and percentages per category.
    Categorical,
    /// Mean and sample standard deviation.
    Continuous,
}

impl fmt::Display for Analysis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Categorical => "categorical",
            Self::Continuous => "continuous",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Condition {
    pub(super) field: FieldName,
    pub(super) equals: Value,
}

/// Why a report cannot be bound to a dataset definition.
///
/// Every variant names the dataset field whose absence or type broke the
/// binding.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IncompatibleReport {
    /// A variable or condition names a field the dataset lacks.
    #[error("dataset has no field {field}")]
    UnknownField { field: FieldName },
    /// The analysis does not apply to the field's type.
    #[error("{analysis} analysis cannot summarize {kind} field {field}")]
    IncompatibleAnalysis {
        field: FieldName,
        analysis: Analysis,
        kind: FieldType,
    },
    /// A boolean field declares category ordering; booleans order true, false.
    #[error("boolean field {field} cannot declare a category order")]
    CategoryOrderOnBoolean { field: FieldName },
    /// An `include_when` value does not have the condition field's type.
    ///
    /// `field` is the condition field.
    #[error("condition on field {field} must compare with a {expected} value")]
    ConditionTypeMismatch {
        field: FieldName,
        expected: FieldType,
    },
}

/// Coarse classification of an [`IncompatibleReport`].
///
/// Serializes in camelCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IncompatibilityKind {
    /// The dataset lacks a field the report names.
    UnknownField,
    /// A field exists but its type no longer fits the report.
    IncompatibleType,
}

impl IncompatibleReport {
    /// The dataset field whose absence or type broke the binding.
    pub fn field(&self) -> &FieldName {
        match self {
            Self::UnknownField { field }
            | Self::IncompatibleAnalysis { field, .. }
            | Self::CategoryOrderOnBoolean { field }
            | Self::ConditionTypeMismatch { field, .. } => field,
        }
    }

    /// Classifies the incompatibility.
    pub fn kind(&self) -> IncompatibilityKind {
        match self {
            Self::UnknownField { .. } => IncompatibilityKind::UnknownField,
            Self::IncompatibleAnalysis { .. }
            | Self::CategoryOrderOnBoolean { .. }
            | Self::ConditionTypeMismatch { .. } => IncompatibilityKind::IncompatibleType,
        }
    }
}

impl ReportDefinition {
    /// Unique report name.
    pub fn name(&self) -> &ReportName {
        &self.name
    }

    /// Name of the dataset the report summarizes.
    pub fn dataset(&self) -> &DatasetName {
        &self.dataset
    }

    /// Report title shown above the summary.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Report description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Variables in display order.
    pub fn variables(&self) -> &[Variable] {
        &self.variables
    }

    /// Checks the report's fields and types against `dataset`.
    ///
    /// `dataset` may be a snapshot's captured definition rather than the
    /// deployed one; the result is valid for that definition only.
    ///
    /// # Errors
    ///
    /// Returns the [`IncompatibleReport`] of the first variable that does not
    /// fit `dataset`.
    ///
    /// # Panics
    ///
    /// Panics if `dataset` is not the dataset the report names; callers pair
    /// a report with the dataset it names.
    pub fn bind(&self, dataset: &DatasetDefinition) -> Result<BoundReport<'_>, IncompatibleReport> {
        assert_eq!(
            dataset.name(),
            &self.dataset,
            "report {} must bind to its own dataset",
            self.name,
        );
        let kinds = self
            .variables
            .iter()
            .map(|variable| variable.bind(dataset))
            .collect::<Result<_, _>>()?;
        Ok(BoundReport {
            definition: self,
            kinds,
        })
    }
}

/// A report checked against one dataset definition, ready to summarize rows.
///
/// Obtainable only through [`ReportDefinition::bind`].
#[derive(Debug, Clone, PartialEq)]
pub struct BoundReport<'a> {
    definition: &'a ReportDefinition,
    /// Declared type of each variable's field, aligned with the variables.
    kinds: Vec<FieldType>,
}

impl BoundReport<'_> {
    /// The bound report definition.
    pub fn definition(&self) -> &ReportDefinition {
        self.definition
    }

    /// Summarizes snapshot `rows` by arm.
    ///
    /// Columns are the configured `arms` in order, including arms with no
    /// rows. Arms found only in `rows` follow, named as stored, in order of
    /// their first row by randomization ID.
    pub fn summarize(&self, arms: &[Arm], rows: &[DatasetRow]) -> Report {
        summary::summarize(self, arms, rows)
    }

    /// Returns each variable with its field's declared type.
    pub(super) fn typed_variables(&self) -> impl Iterator<Item = (&Variable, FieldType)> {
        self.definition
            .variables
            .iter()
            .zip(self.kinds.iter().copied())
    }
}

impl Variable {
    /// Dataset field this variable summarizes.
    pub fn field(&self) -> &FieldName {
        &self.field
    }

    /// Display label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// How the field is summarized.
    pub fn analysis(&self) -> Analysis {
        self.analysis
    }

    /// Runs the checks that do not need a dataset.
    fn check(&self) -> Result<(), DefinitionError> {
        if self.label.trim().is_empty() {
            return Err(DefinitionError::BlankLabel {
                field: self.field.clone(),
            });
        }
        self.check_unit()?;
        self.check_categories()?;
        self.check_decimals()?;
        if self.include_when.is_some() && self.analysis == Analysis::Categorical {
            return Err(DefinitionError::ConditionOnCategorical {
                field: self.field.clone(),
            });
        }
        Ok(())
    }

    /// Checks the variable against `dataset`, returning its field's type.
    fn bind(&self, dataset: &DatasetDefinition) -> Result<FieldType, IncompatibleReport> {
        let kind = dataset
            .field(&self.field)
            .ok_or_else(|| IncompatibleReport::UnknownField {
                field: self.field.clone(),
            })?
            .kind();
        self.check_analysis(kind)?;
        if kind == FieldType::Boolean && !self.category_order.is_empty() {
            return Err(IncompatibleReport::CategoryOrderOnBoolean {
                field: self.field.clone(),
            });
        }
        if let Some(condition) = &self.include_when {
            condition.bind(dataset)?;
        }
        Ok(kind)
    }

    fn check_unit(&self) -> Result<(), DefinitionError> {
        let Some(unit) = &self.unit else {
            return Ok(());
        };
        if unit.trim().is_empty() {
            return Err(DefinitionError::BlankUnit {
                field: self.field.clone(),
            });
        }
        if self.analysis == Analysis::Categorical {
            return Err(DefinitionError::UnitOnCategorical {
                field: self.field.clone(),
            });
        }
        Ok(())
    }

    fn check_analysis(&self, kind: FieldType) -> Result<(), IncompatibleReport> {
        match (self.analysis, kind) {
            (Analysis::Continuous, FieldType::Number)
            | (Analysis::Categorical, FieldType::String | FieldType::Boolean) => Ok(()),
            (Analysis::Continuous, FieldType::String | FieldType::Boolean)
            | (Analysis::Categorical, FieldType::Number) => {
                Err(IncompatibleReport::IncompatibleAnalysis {
                    field: self.field.clone(),
                    analysis: self.analysis,
                    kind,
                })
            }
        }
    }

    fn check_categories(&self) -> Result<(), DefinitionError> {
        let mut seen = BTreeSet::new();
        if let Some(category) = self.category_order.iter().find(|c| !seen.insert(*c)) {
            return Err(DefinitionError::DuplicateCategory {
                field: self.field.clone(),
                category: category.clone(),
            });
        }
        if !self.category_order.is_empty() && self.analysis == Analysis::Continuous {
            return Err(DefinitionError::CategoriesOnContinuous {
                field: self.field.clone(),
            });
        }
        Ok(())
    }

    fn check_decimals(&self) -> Result<(), DefinitionError> {
        let Some(found) = self.decimals else {
            return Ok(());
        };
        if self.analysis == Analysis::Categorical {
            return Err(DefinitionError::DecimalsOnCategorical {
                field: self.field.clone(),
            });
        }
        if found > MAX_DECIMALS {
            return Err(DefinitionError::InvalidDecimals {
                field: self.field.clone(),
                found,
            });
        }
        Ok(())
    }
}

impl Condition {
    fn bind(&self, dataset: &DatasetDefinition) -> Result<(), IncompatibleReport> {
        let expected = dataset
            .field(&self.field)
            .ok_or_else(|| IncompatibleReport::UnknownField {
                field: self.field.clone(),
            })?
            .kind();
        let matches_type = match expected {
            FieldType::Boolean => self.equals.is_boolean(),
            FieldType::Number => self.equals.is_number(),
            FieldType::String => self.equals.is_string(),
        };
        if matches_type {
            Ok(())
        } else {
            Err(IncompatibleReport::ConditionTypeMismatch {
                field: self.field.clone(),
                expected,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn report_json() -> Value {
        json!({
            "version": 1,
            "name": "outcomes",
            "dataset": "outcomes",
            "title": "Outcomes",
            "description": "By arm",
            "variables": [
                {
                    "field": "los", "label": "LOS", "analysis": "continuous",
                    "unit": "days", "decimals": 1, "note": null,
                    "category_order": [],
                    "include_when": {"field": "readmitted", "equals": true}
                },
                {
                    "field": "readmitted", "label": "Readmission", "analysis": "categorical",
                    "unit": null, "decimals": null, "note": null,
                    "category_order": [],
                    "include_when": null
                },
                {
                    "field": "disposition", "label": "Disposition", "analysis": "categorical",
                    "unit": null, "decimals": null, "note": null,
                    "category_order": ["home", "snf"],
                    "include_when": null
                }
            ]
        })
    }

    fn report_error(edit: impl FnOnce(&mut Value)) -> DefinitionError {
        let mut value = report_json();
        edit(&mut value);
        let raw: RawReport = serde_json::from_value(value).expect("raw shape");
        ReportDefinition::try_from(raw).expect_err("report should be rejected")
    }

    #[test]
    fn valid_report_is_accepted() {
        let raw: RawReport = serde_json::from_value(report_json()).expect("raw shape");

        let report = ReportDefinition::try_from(raw).expect("valid report");

        assert_eq!(report.variables().len(), 3);
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let error = report_error(|v| v["version"] = json!(3));

        assert!(matches!(
            error,
            DefinitionError::UnsupportedVersion { found: 3 }
        ));
    }

    #[test]
    fn invalid_report_name_is_rejected() {
        let error = report_error(|v| v["name"] = json!("Bad_Name"));

        assert!(matches!(
            error,
            DefinitionError::InvalidReportName { value } if value == "Bad_Name"
        ));
    }

    #[test]
    fn invalid_dataset_reference_is_rejected() {
        let error = report_error(|v| v["dataset"] = json!("Bad Name"));

        assert!(matches!(
            error,
            DefinitionError::InvalidDatasetName { value } if value == "Bad Name"
        ));
    }

    #[test]
    fn blank_title_is_rejected() {
        let error = report_error(|v| v["title"] = json!(""));

        assert!(matches!(error, DefinitionError::BlankTitle));
    }

    #[test]
    fn report_without_variables_is_rejected() {
        let error = report_error(|v| v["variables"] = json!([]));

        assert!(matches!(error, DefinitionError::NoVariables));
    }

    #[test]
    fn field_summarized_twice_is_rejected() {
        let error = report_error(|v| v["variables"][1]["field"] = json!("disposition"));

        assert!(matches!(
            error,
            DefinitionError::DuplicateReportField { field } if field.as_str() == "disposition"
        ));
    }

    #[test]
    fn blank_label_is_rejected() {
        let error = report_error(|v| v["variables"][0]["label"] = json!(" "));

        assert!(matches!(
            error,
            DefinitionError::BlankLabel { field } if field.as_str() == "los"
        ));
    }

    #[test]
    fn blank_unit_is_rejected() {
        let error = report_error(|v| v["variables"][0]["unit"] = json!(" "));

        assert!(matches!(
            error,
            DefinitionError::BlankUnit { field } if field.as_str() == "los"
        ));
    }

    #[test]
    fn unit_on_categorical_variable_is_rejected() {
        let error = report_error(|v| v["variables"][1]["unit"] = json!("days"));

        assert!(matches!(
            error,
            DefinitionError::UnitOnCategorical { field } if field.as_str() == "readmitted"
        ));
    }

    #[test]
    fn duplicate_category_order_is_rejected() {
        let error = report_error(|v| v["variables"][2]["category_order"] = json!(["a", "a"]));

        assert!(matches!(
            error,
            DefinitionError::DuplicateCategory { category, .. } if category == "a"
        ));
    }

    #[test]
    fn category_order_on_continuous_variable_is_rejected() {
        let error = report_error(|v| v["variables"][0]["category_order"] = json!(["a"]));

        assert!(matches!(
            error,
            DefinitionError::CategoriesOnContinuous { .. }
        ));
    }

    #[test]
    fn decimals_on_categorical_variable_are_rejected() {
        let error = report_error(|v| v["variables"][1]["decimals"] = json!(2));

        assert!(matches!(
            error,
            DefinitionError::DecimalsOnCategorical { field } if field.as_str() == "readmitted"
        ));
    }

    #[test]
    fn more_than_six_decimals_are_rejected() {
        let error = report_error(|v| v["variables"][0]["decimals"] = json!(7));

        assert!(matches!(
            error,
            DefinitionError::InvalidDecimals { found: 7, .. }
        ));
    }

    #[test]
    fn condition_on_categorical_variable_is_rejected() {
        let error = report_error(|v| {
            v["variables"][1]["include_when"] = json!({"field": "readmitted", "equals": true});
        });

        assert!(matches!(
            error,
            DefinitionError::ConditionOnCategorical { field } if field.as_str() == "readmitted"
        ));
    }
}
