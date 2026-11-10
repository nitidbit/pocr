use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DatasetDefinition, DatasetName, DatasetRow};
use crate::{environment::Environment, ports::RepositoryError};

/// Refresh state of an attachment in one environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatasetStatus {
    /// No refresh has started.
    Pending,
    /// A refresh is in progress.
    Running,
    /// The latest refresh published a snapshot.
    Succeeded,
    /// The latest refresh failed; the previous snapshot, if any, stays visible.
    Failed,
}

impl DatasetStatus {
    /// Parses the lowercase stored form, returning `None` for any other string.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// A dataset attached to a study, with its state in one environment.
#[derive(Debug, Clone)]
pub struct Attachment {
    /// Attachment identifier; a re-attached dataset gets a new one.
    pub id: Uuid,
    /// Owning study.
    pub study_id: Uuid,
    /// Dataset name.
    pub name: DatasetName,
    /// Title from the latest published run's definition, else the name.
    pub title: String,
    /// Description from the latest published run's definition; may be empty.
    pub description: String,
    /// State of the latest run in the requested environment.
    pub status: DatasetStatus,
    /// The latest published snapshot in the requested environment, if any.
    pub snapshot: Option<PublishedSummary>,
}

/// What an attachment's latest published snapshot holds.
#[derive(Debug, Clone)]
pub struct PublishedSummary {
    /// When the publishing run completed.
    pub completed_at: DateTime<Utc>,
    /// Rows in the snapshot.
    pub row_count: u64,
    /// Fingerprint of the dataset definition the snapshot was produced under.
    pub fingerprint: String,
}

/// A published dataset snapshot with the definition it was produced under.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Run that produced the snapshot.
    pub run_id: Uuid,
    /// When the run was published.
    pub completed_at: DateTime<Utc>,
    /// Status of the attachment's latest run, which may be newer than this snapshot.
    pub status: DatasetStatus,
    /// Definition captured when the run began.
    pub definition: DatasetDefinition,
    /// One row per randomization in the captured cohort.
    pub rows: Vec<DatasetRow>,
}

impl Snapshot {
    /// Fingerprint of the definition the snapshot was produced under.
    pub fn fingerprint(&self) -> &str {
        self.definition.fingerprint()
    }
}

/// Storage port for dataset attachments and published snapshots.
#[async_trait]
pub trait DatasetRepository: Send + Sync {
    /// Lists a study's attachments with their state in `environment`.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Internal`] if the store fails.
    async fn list(
        &self,
        study: Uuid,
        environment: Environment,
    ) -> Result<Vec<Attachment>, RepositoryError>;

    /// Attaches a dataset to a study for both environments. Attaching an
    /// already attached dataset changes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Internal`] if the store fails.
    async fn attach(
        &self,
        study: Uuid,
        definition: &DatasetDefinition,
    ) -> Result<(), RepositoryError>;

    /// Detaches a dataset from a study, deleting its runs and snapshots in both
    /// environments. A later attach creates a new attachment, so runs started
    /// before the detach can no longer publish.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Internal`] if the store fails.
    async fn detach(&self, study: Uuid, name: &DatasetName) -> Result<(), RepositoryError>;

    /// Reads the latest published snapshot, or `None` if the dataset has
    /// never published in `environment`.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Internal`] if the store fails or the stored
    /// definition cannot be decoded.
    async fn snapshot(
        &self,
        study: Uuid,
        name: &DatasetName,
        environment: Environment,
    ) -> Result<Option<Snapshot>, RepositoryError>;
}
