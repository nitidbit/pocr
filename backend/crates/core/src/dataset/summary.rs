use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::{Number, Value};
use uuid::Uuid;

use crate::random::Arm;

use super::definition::{FieldName, FieldType};
use super::record::DatasetRow;
use super::report::{Analysis, BoundReport, Condition, Variable};

/// Decimal places for a continuous variable that declares none.
///
/// The datasets README documents this default as part of the report contract.
const DEFAULT_DECIMALS: u8 = 1;

/// A report ready to serialize: one column per arm and one entry per variable.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Report {
    /// Report title from the definition.
    pub title: String,
    /// Report description from the definition.
    pub description: String,
    /// Arm columns: configured arms in order, then arms found only in the rows.
    pub arms: Vec<ReportArm>,
    /// One entry per report variable, in definition order.
    pub fields: Vec<ReportField>,
}

/// One arm column with the number of snapshot rows assigned to it.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct ReportArm {
    /// Arm identifier.
    pub id: Uuid,
    /// Configured arm name; the stored name for arms no longer configured.
    pub name: String,
    /// Snapshot rows in this arm.
    pub count: usize,
}

/// One summarized variable; every per-arm vector is aligned with [`Report::arms`].
///
/// Serializes with an `analysis` tag; `field` serializes as `id` and `label`
/// as `name`.
#[derive(Debug, Serialize)]
#[serde(tag = "analysis", rename_all = "lowercase")]
pub enum ReportField {
    /// Counts per category.
    #[non_exhaustive]
    Categorical {
        /// Dataset field name.
        #[serde(rename = "id")]
        field: FieldName,
        /// Display label, rendered verbatim.
        #[serde(rename = "name")]
        label: String,
        /// Observed categories only, in display order.
        categories: Vec<Category>,
    },
    /// Mean, sample standard deviation, and eligible-row count.
    #[serde(rename_all = "camelCase")]
    #[non_exhaustive]
    Continuous {
        /// Dataset field name.
        #[serde(rename = "id")]
        field: FieldName,
        /// Display label, rendered verbatim.
        #[serde(rename = "name")]
        label: String,
        /// Display unit, if any.
        unit: Option<String>,
        /// Footnote shown with the variable, if any.
        note: Option<String>,
        /// Display precision; defaults to 1.
        decimals: u8,
        /// Mean per arm; `None` when the arm has no eligible values.
        means: Vec<Option<f64>>,
        /// Sample standard deviation per arm; `None` below two values.
        standard_deviations: Vec<Option<f64>>,
        /// Eligible non-null values per arm; rows excluded by the
        /// `include_when` condition or holding null are not counted.
        denominators: Vec<usize>,
    },
}

/// One category of a categorical variable with its count in each arm.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Category {
    /// The stored value; `None` marks rows with no value.
    ///
    /// Boolean fields use the strings `"true"` and `"false"`.
    pub id: Option<String>,
    /// Display label: `Yes`/`No` for booleans, `Missing` for `None`, otherwise the value.
    pub label: String,
    /// Rows in the category per arm, aligned with [`Report::arms`].
    pub counts: Vec<usize>,
}

pub(super) fn summarize(report: &BoundReport<'_>, arms: &[Arm], rows: &[DatasetRow]) -> Report {
    let groups = ArmGroups::new(arms, rows);
    let fields = report
        .typed_variables()
        .map(|(variable, kind)| match variable.analysis() {
            Analysis::Categorical => summarize_categorical(variable, kind, &groups),
            Analysis::Continuous => summarize_continuous(variable, &groups),
        })
        .collect();
    Report {
        title: report.definition().title().to_owned(),
        description: report.definition().description().to_owned(),
        arms: groups.report_arms(),
        fields,
    }
}

struct ArmColumn<'a> {
    id: Uuid,
    name: String,
    rows: Vec<&'a DatasetRow>,
}

struct ArmGroups<'a>(Vec<ArmColumn<'a>>);

impl<'a> ArmGroups<'a> {
    fn new(arms: &[Arm], rows: &'a [DatasetRow]) -> Self {
        let mut columns: Vec<ArmColumn<'a>> = arms
            .iter()
            .map(|arm| ArmColumn {
                id: arm.id,
                name: arm.name.clone(),
                rows: Vec::new(),
            })
            .collect();
        let mut ordered: Vec<&DatasetRow> = rows.iter().collect();
        ordered.sort_by_key(|row| row.randomization_id);
        for row in ordered {
            let index = columns
                .iter()
                .position(|column| column.id == row.arm_id)
                .unwrap_or_else(|| {
                    columns.push(ArmColumn {
                        id: row.arm_id,
                        name: row.arm_name.clone(),
                        rows: Vec::new(),
                    });
                    columns.len() - 1
                });
            columns[index].rows.push(row);
        }
        Self(columns)
    }

    fn report_arms(&self) -> Vec<ReportArm> {
        self.0
            .iter()
            .map(|column| ReportArm {
                id: column.id,
                name: column.name.clone(),
                count: column.rows.len(),
            })
            .collect()
    }

    fn per_arm<T>(&self, summarize: impl Fn(&[&DatasetRow]) -> T) -> Vec<T> {
        self.0
            .iter()
            .map(|column| summarize(&column.rows))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum CategoryValue {
    Flag(bool),
    Text(String),
    Missing,
}

impl CategoryValue {
    fn of(row: &DatasetRow, field: &FieldName, kind: FieldType) -> Self {
        let value = row.values.get(field);
        match kind {
            FieldType::Boolean => value
                .and_then(Value::as_bool)
                .map_or(Self::Missing, Self::Flag),
            FieldType::String => value
                .and_then(Value::as_str)
                .map_or(Self::Missing, |text| Self::Text(text.to_owned())),
            FieldType::Number => {
                unreachable!("report validation rejects categorical number field {field}")
            }
        }
    }

    fn id(&self) -> Option<String> {
        match self {
            Self::Flag(flag) => Some(flag.to_string()),
            Self::Text(text) => Some(text.clone()),
            Self::Missing => None,
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Flag(true) => "Yes".to_owned(),
            Self::Flag(false) => "No".to_owned(),
            Self::Text(text) => text.clone(),
            Self::Missing => "Missing".to_owned(),
        }
    }

    /// Booleans order true, false; listed strings follow `category_order`,
    /// unlisted ones follow lexically; missing is always last.
    fn sort_key(&self, category_order: &[String]) -> (u8, usize, String) {
        match self {
            Self::Flag(flag) => (0, usize::from(!flag), String::new()),
            Self::Text(text) => {
                let position = category_order
                    .iter()
                    .position(|listed| listed == text)
                    .unwrap_or(usize::MAX);
                (1, position, text.clone())
            }
            Self::Missing => (2, 0, String::new()),
        }
    }
}

fn summarize_categorical(
    variable: &Variable,
    kind: FieldType,
    groups: &ArmGroups<'_>,
) -> ReportField {
    let tallies = groups.per_arm(|rows| {
        let mut tally: HashMap<CategoryValue, usize> = HashMap::new();
        for row in rows {
            *tally
                .entry(CategoryValue::of(row, &variable.field, kind))
                .or_default() += 1;
        }
        tally
    });
    let observed: HashSet<&CategoryValue> = tallies.iter().flat_map(HashMap::keys).collect();
    let mut observed: Vec<&CategoryValue> = observed.into_iter().collect();
    observed.sort_by_cached_key(|value| value.sort_key(&variable.category_order));
    let categories = observed
        .into_iter()
        .map(|value| Category {
            id: value.id(),
            label: value.label(),
            counts: tallies
                .iter()
                .map(|tally| tally.get(value).copied().unwrap_or(0))
                .collect(),
        })
        .collect();
    ReportField::Categorical {
        field: variable.field.clone(),
        label: variable.label.clone(),
        categories,
    }
}

fn is_eligible(condition: Option<&Condition>, row: &DatasetRow) -> bool {
    condition.is_none_or(|condition| {
        row.values
            .get(&condition.field)
            .is_some_and(|value| condition_equals(value, &condition.equals))
    })
}

fn summarize_continuous(variable: &Variable, groups: &ArmGroups<'_>) -> ReportField {
    let moments = groups.per_arm(|rows| {
        let values: Vec<f64> = rows
            .iter()
            .filter(|row| is_eligible(variable.include_when.as_ref(), row))
            .filter_map(|row| row.values.get(&variable.field).and_then(Value::as_f64))
            .collect();
        Moments::of(&values)
    });
    ReportField::Continuous {
        field: variable.field.clone(),
        label: variable.label.clone(),
        unit: variable.unit.clone(),
        note: variable.note.clone(),
        decimals: variable.decimals.unwrap_or(DEFAULT_DECIMALS),
        means: moments.iter().map(|m| m.mean).collect(),
        standard_deviations: moments.iter().map(|m| m.standard_deviation).collect(),
        denominators: moments.iter().map(|m| m.count).collect(),
    }
}

/// Compares JSON values, treating numbers by value so `1` equals `1.0`.
fn condition_equals(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => match (integer(a), integer(b)) {
            (Some(a), Some(b)) => a == b,
            (Some(whole), None) => float_equals_integer(b, whole),
            (None, Some(whole)) => float_equals_integer(a, whole),
            (None, None) => a.as_f64() == b.as_f64(),
        },
        _ => left == right,
    }
}

fn integer(number: &Number) -> Option<i128> {
    number
        .as_i64()
        .map(i128::from)
        .or_else(|| number.as_u64().map(i128::from))
}

fn float_equals_integer(float: &Number, whole: i128) -> bool {
    // A whole f64 below 2^127 casts exactly; larger ones saturate and equal no i64/u64.
    float
        .as_f64()
        .is_some_and(|f| f.fract() == 0.0 && f as i128 == whole)
}

struct Moments {
    mean: Option<f64>,
    standard_deviation: Option<f64>,
    count: usize,
}

impl Moments {
    /// Mean and sample standard deviation (Welford); the SD needs two values.
    fn of(values: &[f64]) -> Self {
        let mut mean = 0.0;
        let mut m2 = 0.0;
        for (i, x) in values.iter().enumerate() {
            let delta = x - mean;
            mean += delta / (i + 1) as f64;
            m2 += delta * (x - mean);
        }
        Self {
            mean: (!values.is_empty()).then_some(mean),
            standard_deviation: (values.len() >= 2)
                .then(|| (m2 / (values.len() - 1) as f64).max(0.0).sqrt()),
            count: values.len(),
        }
    }
}
