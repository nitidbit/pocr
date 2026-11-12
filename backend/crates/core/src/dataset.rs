//! Server-managed datasets: validated definitions, procedure results,
//! arm-grouped reports, and the snapshot persistence boundary.
//!
//! A dataset is a stored procedure whose results are captured per study as
//! snapshots. The types fit together as follows:
//!
//! - **Definitions.** A [`DatasetDefinition`] (from `dataset.yaml`) carries a
//!   fingerprint of its canonical form. A [`ReportDefinition`] (from
//!   `report.yaml`) names one dataset and is valid on its own. A [`Catalog`]
//!   holds the deployed [`DatasetRegistry`] and the reports bound to it,
//!   rejecting a report whose dataset is missing or whose fields and types do
//!   not fit.
//! - **Refresh.** Each randomization becomes a [`ContextRecord`];
//!   [`batches`] bounds how many go to one call of the dataset's
//!   [`Procedure`], and [`DatasetDefinition::parse_results`] turns the raw
//!   [`ResultRow`]s into [`DatasetRecord`]s or rejects the result with a
//!   [`ResultsError`].
//! - **Presentation.** A [`DatasetRepository`] serves each study's
//!   [`Attachment`]s and its published [`Snapshot`] of [`DatasetRow`]s, always
//!   with the [`DatasetDefinition`] the snapshot was produced under.
//!   [`ReportDefinition::bind`] checks a deployed report against that captured
//!   definition, and [`BoundReport::summarize`] groups the rows by arm into a
//!   [`Report`]. A report that no longer binds is an [`IncompatibleReport`].
//!
//! Every validated type is a proof of validity: values are checked once while
//! deserializing or constructing, and downstream code relies on that. Stored
//! definitions deserialize through the same checks. No SQL text is accepted; a
//! dataset names its procedure only by validated [`SqlIdentifier`]s.

mod catalog;
mod column;
mod definition;
mod record;
mod report;
mod repository;
mod summary;

pub use catalog::{Catalog, CatalogError, DatasetRegistry};
pub use column::SystemColumn;
pub use definition::{
    ColumnName, DatasetDefinition, DatasetName, DefinitionError, Field, FieldName, FieldType,
    IdentifierRole, Procedure, Source, SqlIdentifier,
};
pub use record::{
    BatchError, ContextRecord, DatasetRecord, DatasetRow, MAX_BATCH_BYTES, MAX_BATCH_RECORDS,
    ResultRow, ResultsError, Values, batches,
};
pub use report::{
    Analysis, BoundReport, IncompatibilityKind, IncompatibleReport, ReportDefinition, ReportName,
    Variable,
};
pub use repository::{Attachment, DatasetRepository, DatasetStatus, PublishedSummary, Snapshot};
pub use summary::{Category, Report, ReportArm, ReportField};
