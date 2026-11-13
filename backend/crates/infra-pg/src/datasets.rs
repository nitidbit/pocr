//! Dataset persistence: attachments and published snapshots, plus the
//! refresh sessions that produce them.
//!
//! [`PgDatasetRepository`] implements [`DatasetRepository`] for the HTTP API
//! and exposes [`refresh_targets`](PgDatasetRepository::refresh_targets) and
//! [`try_lock`](PgDatasetRepository::try_lock) for the sync job. Snapshots are
//! staged under a run ID and become visible only on
//! [`RefreshRun::publish`].

mod error;
mod refresh;

use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use pocr_core::{
    dataset::{
        Attachment, DatasetDefinition, DatasetName, DatasetRepository, DatasetRow, DatasetStatus,
        PublishedSummary, Snapshot,
    },
    environment::Environment,
    ports::RepositoryError,
};

pub use error::{DatabaseFailure, RefreshStoreError};
pub use refresh::{RefreshLock, RefreshRun, RefreshTarget};

/// PostgreSQL-backed dataset store.
#[derive(Clone)]
pub struct PgDatasetRepository {
    pool: PgPool,
}

impl PgDatasetRepository {
    /// Create a repository over `pool`.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn repository_error(error: RefreshStoreError) -> RepositoryError {
    match &error {
        RefreshStoreError::Database { operation, .. } => {
            tracing::error!(operation, error_message = %error, "dataset persistence failed");
        }
        RefreshStoreError::CorruptRun { run_id, source } => {
            tracing::error!(
                %run_id,
                error_message = %error,
                error_source = %source,
                "dataset persistence failed"
            );
        }
        RefreshStoreError::CorruptTarget { attachment_id } => {
            tracing::error!(%attachment_id, error_message = %error, "dataset persistence failed");
        }
        RefreshStoreError::StaleRun { .. } | RefreshStoreError::OutsideCohort { .. } => {
            tracing::error!(error_message = %error, "dataset persistence failed");
        }
    }
    RepositoryError::Internal("dataset persistence failed".into())
}

fn database_error(operation: &'static str) -> impl FnOnce(sqlx::Error) -> RepositoryError {
    move |error| repository_error(RefreshStoreError::database(operation, &error))
}

#[async_trait]
impl DatasetRepository for PgDatasetRepository {
    async fn list(
        &self,
        study: Uuid,
        env: Environment,
    ) -> Result<Vec<Attachment>, RepositoryError> {
        let rows = sqlx::query(include_str!("datasets/list_attachments.sql"))
            .bind(study)
            .bind(env.as_str())
            .fetch_all(&self.pool)
            .await
            .map_err(database_error("list attachments"))?;
        rows.into_iter()
            .map(attachment_from_row)
            .collect::<Result<_, sqlx::Error>>()
            .map_err(database_error("list attachments"))
    }

    async fn attach(
        &self,
        study: Uuid,
        definition: &DatasetDefinition,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO study_datasets(id,study_id,name)
            VALUES ($1,$2,$3)
            ON CONFLICT (study_id,name) DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(study)
        .bind(definition.name().as_ref())
        .execute(&self.pool)
        .await
        .map_err(database_error("attach dataset"))?;
        Ok(())
    }

    async fn detach(&self, study: Uuid, name: &DatasetName) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM study_datasets WHERE study_id=$1 AND name=$2")
            .bind(study)
            .bind(name.as_ref())
            .execute(&self.pool)
            .await
            .map_err(database_error("detach dataset"))?;
        Ok(())
    }

    async fn snapshot(
        &self,
        study: Uuid,
        name: &DatasetName,
        env: Environment,
    ) -> Result<Option<Snapshot>, RepositoryError> {
        // Both reads share one repeatable-read view so a concurrent prune cannot
        // remove the run's rows between the metadata and row queries.
        let operation = "read snapshot";
        let mut tx = self
            .pool
            .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .await
            .map_err(database_error(operation))?;
        let Some(row) = sqlx::query(include_str!("datasets/snapshot.sql"))
            .bind(study)
            .bind(name.as_ref())
            .bind(env.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(database_error(operation))?
        else {
            return Ok(None);
        };
        let mut snapshot = snapshot_from_row(&row)?;
        snapshot.rows = sqlx::query(include_str!("datasets/snapshot_rows.sql"))
            .bind(snapshot.run_id)
            .try_map(|row: sqlx::postgres::PgRow| dataset_row_from_row(&row))
            .fetch_all(&mut *tx)
            .await
            .map_err(database_error(operation))?;
        tx.commit().await.map_err(database_error(operation))?;
        Ok(Some(snapshot))
    }
}

fn attachment_from_row(row: sqlx::postgres::PgRow) -> Result<Attachment, sqlx::Error> {
    let has_snapshot: bool = row.try_get("has_snapshot")?;
    let snapshot = if has_snapshot {
        Some(published_summary(&row)?)
    } else {
        None
    };
    Ok(Attachment {
        id: row.try_get("id")?,
        study_id: row.try_get("study_id")?,
        name: DatasetName::parse(row.try_get::<String, _>("name")?)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        status: status_from_row(&row)?,
        snapshot,
    })
}

fn published_summary(row: &sqlx::postgres::PgRow) -> Result<PublishedSummary, sqlx::Error> {
    let row_count: i64 = row.try_get("row_count")?;
    Ok(PublishedSummary {
        completed_at: row.try_get("completed_at")?,
        row_count: u64::try_from(row_count).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        fingerprint: row.try_get("fingerprint")?,
    })
}

fn status_from_row(row: &sqlx::postgres::PgRow) -> Result<DatasetStatus, sqlx::Error> {
    let status: String = row.try_get("status")?;
    DatasetStatus::parse(&status)
        .ok_or_else(|| sqlx::Error::Decode(format!("invalid dataset status {status:?}").into()))
}

/// Decode run metadata; `rows` is filled by the caller.
fn snapshot_from_row(row: &sqlx::postgres::PgRow) -> Result<Snapshot, RepositoryError> {
    let decode = || database_error("read snapshot");
    let run_id: Uuid = row.try_get("id").map_err(decode())?;
    let definition_json = row
        .try_get::<sqlx::types::Json<serde_json::Value>, _>("definition")
        .map_err(decode())?
        .0;
    let definition = serde_json::from_value::<DatasetDefinition>(definition_json)
        .map_err(|source| repository_error(RefreshStoreError::CorruptRun { run_id, source }))?;
    let read = || -> Result<Snapshot, sqlx::Error> {
        Ok(Snapshot {
            run_id,
            completed_at: row.try_get("completed_at")?,
            status: status_from_row(row)?,
            definition,
            rows: Vec::new(),
        })
    };
    read().map_err(decode())
}

fn dataset_row_from_row(row: &sqlx::postgres::PgRow) -> Result<DatasetRow, sqlx::Error> {
    Ok(DatasetRow {
        randomization_id: row.try_get("randomization_id")?,
        arm_id: row.try_get("arm_id")?,
        arm_name: row.try_get("arm_name")?,
        values: row.try_get::<sqlx::types::Json<_>, _>("values")?.0,
    })
}
