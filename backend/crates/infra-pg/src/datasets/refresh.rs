//! Refresh sessions: a per-attachment lock, then a staged run that is
//! published or failed.
//!
//! ```text
//!   try_lock ──▶ RefreshLock ──begin()──▶ RefreshRun ──publish()/fail()──▶ (consumed)
//! ```

use std::collections::BTreeMap;

use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};
use uuid::Uuid;

use pocr_core::{
    dataset::{ContextRecord, DatasetDefinition, DatasetName, DatasetRecord, MAX_BATCH_RECORDS},
    environment::Environment,
};

use super::{PgDatasetRepository, RefreshStoreError};

/// An attachment that needs refreshing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshTarget {
    /// Attachment to refresh; one per study and dataset.
    pub attachment_id: Uuid,
    /// Dataset the attachment points at.
    pub dataset_name: DatasetName,
}

impl PgDatasetRepository {
    /// List every attachment, ordered by ID.
    ///
    /// An attachment with an invalid stored name yields
    /// [`RefreshStoreError::CorruptTarget`] in its own slot, so the rest can
    /// still be refreshed.
    ///
    /// # Errors
    ///
    /// Returns [`RefreshStoreError::Database`] if the query fails.
    pub async fn refresh_targets(
        &self,
    ) -> Result<Vec<Result<RefreshTarget, RefreshStoreError>>, RefreshStoreError> {
        let rows: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id, name FROM study_datasets ORDER BY id")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| RefreshStoreError::database("list refresh targets", &e))?;
        Ok(rows
            .into_iter()
            .map(|(attachment_id, name)| parse_target(attachment_id, name))
            .collect())
    }

    /// Try to take the refresh lock for an attachment in one environment.
    ///
    /// The lock lives on a dedicated connection and is released when the
    /// returned session is dropped, published, or failed.
    ///
    /// # Errors
    ///
    /// Returns [`RefreshStoreError::Database`] if no connection can be acquired
    /// or the lock query fails. Returns `Ok(None)` when another session holds the lock.
    pub async fn try_lock(
        &self,
        attachment_id: Uuid,
        environment: Environment,
    ) -> Result<Option<RefreshLock>, RefreshStoreError> {
        let operation = "acquire refresh lock";
        let mut connection = self
            .pool
            .acquire()
            .await
            .map_err(|e| RefreshStoreError::database(operation, &e))?
            .detach();
        let is_locked: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1, 0))")
                .bind(format!("dataset:{attachment_id}:{environment}"))
                .fetch_one(&mut connection)
                .await
                .map_err(|e| RefreshStoreError::database(operation, &e))?;
        Ok(is_locked.then_some(RefreshLock {
            connection,
            attachment_id,
            environment,
        }))
    }
}

/// Exclusive right to refresh one attachment in one environment.
#[derive(Debug)]
pub struct RefreshLock {
    connection: PgConnection,
    attachment_id: Uuid,
    environment: Environment,
}

impl RefreshLock {
    /// Start a run: discard abandoned runs, record `definition`, and capture the cohort.
    ///
    /// The lock proves no earlier runner is alive, so runs left `running` or
    /// `failed` are deleted. The cohort is every randomization in the
    /// attachment's study and environment at this moment.
    ///
    /// # Errors
    ///
    /// Returns [`RefreshStoreError::Database`] if any statement fails; the
    /// transaction is rolled back.
    pub async fn begin(
        mut self,
        definition: &DatasetDefinition,
    ) -> Result<RefreshRun, RefreshStoreError> {
        let run_id = Uuid::now_v7();
        self.start(run_id, definition)
            .await
            .map_err(|e| RefreshStoreError::database("begin run", &e))?;
        Ok(RefreshRun {
            connection: self.connection,
            run_id,
            attachment_id: self.attachment_id,
            environment: self.environment,
            cursor: None,
        })
    }

    async fn start(
        &mut self,
        run_id: Uuid,
        definition: &DatasetDefinition,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.connection.begin().await?;
        sqlx::query(
            "DELETE FROM dataset_runs
            WHERE attachment_id=$1 AND environment=$2 AND status IN ('running','failed')",
        )
        .bind(self.attachment_id)
        .bind(self.environment.as_str())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO dataset_runs(id,attachment_id,environment,definition,fingerprint,status)
            VALUES ($1,$2,$3,$4,$5,'running')",
        )
        .bind(run_id)
        .bind(self.attachment_id)
        .bind(self.environment.as_str())
        .bind(sqlx::types::Json(definition))
        .bind(definition.fingerprint())
        .execute(&mut *tx)
        .await?;
        sqlx::query(include_str!("capture_cohort.sql"))
            .bind(run_id)
            .bind(self.attachment_id)
            .bind(self.environment.as_str())
            .execute(&mut *tx)
            .await?;
        tx.commit().await
    }
}

/// A run in progress: a captured cohort being read in pages and staged.
///
/// Dropping a run without [`publish`](Self::publish) or [`fail`](Self::fail)
/// releases the lock and leaves a `running` row that the next
/// [`RefreshLock::begin`] discards.
#[derive(Debug)]
pub struct RefreshRun {
    connection: PgConnection,
    run_id: Uuid,
    attachment_id: Uuid,
    environment: Environment,
    cursor: Option<Uuid>,
}

impl RefreshRun {
    /// Identifier of this run.
    pub fn id(&self) -> Uuid {
        self.run_id
    }

    /// Read the next page of at most [`MAX_BATCH_RECORDS`] cohort records,
    /// ordered by randomization ID.
    ///
    /// Returns an empty page once the cohort is exhausted, or when the run
    /// has been detached or purged; [`publish`](Self::publish) then reports
    /// [`RefreshStoreError::StaleRun`].
    ///
    /// # Errors
    ///
    /// Returns [`RefreshStoreError::Database`] if the query or decoding fails.
    pub async fn next_page(&mut self) -> Result<Vec<ContextRecord>, RefreshStoreError> {
        let limit = i64::try_from(MAX_BATCH_RECORDS).expect("batch size fits in i64");
        let rows = sqlx::query(include_str!("page_cohort.sql"))
            .bind(self.run_id)
            .bind(self.cursor)
            .bind(limit)
            .fetch_all(&mut self.connection)
            .await
            .map_err(|e| RefreshStoreError::database("read cohort", &e))?;
        let page = rows
            .into_iter()
            .map(|row| {
                Ok(ContextRecord {
                    randomization_id: row.try_get("id")?,
                    randomization_time: row.try_get("issued_at")?,
                    context: row
                        .try_get::<sqlx::types::Json<BTreeMap<String, Value>>, _>("context")?
                        .0,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(|e| RefreshStoreError::database("read cohort", &e))?;
        if let Some(last) = page.last() {
            self.cursor = Some(last.randomization_id);
        }
        Ok(page)
    }

    /// Stage procedure results for cohort rows, all or nothing.
    ///
    /// # Errors
    ///
    /// Returns [`RefreshStoreError::StaleRun`] if the run is no longer
    /// `running`, [`RefreshStoreError::OutsideCohort`] if a record is not an
    /// unstaged row of the cohort, and [`RefreshStoreError::Database`] if a
    /// statement fails. Nothing from `records` is kept on error.
    pub async fn stage(&mut self, records: &[DatasetRecord]) -> Result<(), RefreshStoreError> {
        let run_id = self.run_id;
        let mut tx = self
            .connection
            .begin()
            .await
            .map_err(|e| RefreshStoreError::database("stage results", &e))?;
        for record in records {
            let staged = sqlx::query(
                "UPDATE dataset_rows
                SET values=$3
                WHERE run_id=$1 AND randomization_id=$2 AND values IS NULL",
            )
            .bind(run_id)
            .bind(record.randomization_id())
            .bind(sqlx::types::Json(record.values()))
            .execute(&mut *tx)
            .await
            .map_err(|e| RefreshStoreError::database("stage results", &e))?
            .rows_affected();
            if staged != 1 {
                return Err(classify_unstaged(&mut tx, run_id, record).await);
            }
        }
        tx.commit()
            .await
            .map_err(|e| RefreshStoreError::database("stage results", &e))
    }

    /// Make the run's snapshot visible, replacing the previous one, and
    /// return the published row count.
    ///
    /// Older runs are pruned in the same transaction. Consumes the run and
    /// releases the lock.
    ///
    /// # Errors
    ///
    /// Returns [`RefreshStoreError::StaleRun`] if the run was detached,
    /// purged, already finished, or has unstaged rows, and
    /// [`RefreshStoreError::Database`] if a statement fails.
    pub async fn publish(mut self) -> Result<u64, RefreshStoreError> {
        let row = self
            .finish("publish run", include_str!("publish_run.sql"))
            .await?;
        let row_count: i64 = row
            .try_get("row_count")
            .map_err(|e| RefreshStoreError::database("publish run", &e))?;
        u64::try_from(row_count).map_err(|e| {
            RefreshStoreError::database("publish run", &sqlx::Error::Decode(Box::new(e)))
        })
    }

    /// Mark the run failed, keeping the previous snapshot visible.
    ///
    /// Consumes the run and releases the lock.
    ///
    /// # Errors
    ///
    /// Returns [`RefreshStoreError::StaleRun`] if the run was detached,
    /// purged, or already finished, and [`RefreshStoreError::Database`] if a
    /// statement fails.
    pub async fn fail(mut self) -> Result<(), RefreshStoreError> {
        self.finish("fail run", include_str!("fail_run.sql"))
            .await
            .map(|_| ())
    }

    async fn finish(
        &mut self,
        operation: &'static str,
        statement: &str,
    ) -> Result<sqlx::postgres::PgRow, RefreshStoreError> {
        let run_id = self.run_id;
        let database = |e: sqlx::Error| RefreshStoreError::database(operation, &e);
        let mut tx = self.connection.begin().await.map_err(database)?;
        let row = sqlx::query(statement)
            .bind(run_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(database)?
            .ok_or(RefreshStoreError::StaleRun { run_id })?;
        sqlx::query(
            "DELETE FROM dataset_rows
            WHERE run_id IN (
                SELECT id FROM dataset_runs
                WHERE attachment_id=$1 AND environment=$2 AND status='failed'
            )",
        )
        .bind(self.attachment_id)
        .bind(self.environment.as_str())
        .execute(&mut *tx)
        .await
        .map_err(database)?;
        sqlx::query(include_str!("prune_runs.sql"))
            .bind(self.attachment_id)
            .bind(self.environment.as_str())
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(database)?;
        tx.commit().await.map_err(database)?;
        Ok(row)
    }
}

fn parse_target(attachment_id: Uuid, name: String) -> Result<RefreshTarget, RefreshStoreError> {
    Ok(RefreshTarget {
        attachment_id,
        dataset_name: DatasetName::parse(name)
            .map_err(|_| RefreshStoreError::CorruptTarget { attachment_id })?,
    })
}

async fn classify_unstaged(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_id: Uuid,
    record: &DatasetRecord,
) -> RefreshStoreError {
    let is_running: Result<bool, _> = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dataset_runs WHERE id=$1 AND status='running')",
    )
    .bind(run_id)
    .fetch_one(&mut **tx)
    .await;
    match is_running {
        Ok(true) => RefreshStoreError::OutsideCohort {
            run_id,
            randomization_id: record.randomization_id(),
        },
        Ok(false) => RefreshStoreError::StaleRun { run_id },
        Err(error) => RefreshStoreError::database("stage results", &error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_accepts_a_valid_name() {
        let id = Uuid::from_u128(1);

        let target = parse_target(id, "sample-outcomes".into()).unwrap();

        assert_eq!(target.attachment_id, id);
        assert_eq!(target.dataset_name.as_ref(), "sample-outcomes");
    }

    #[test]
    fn parse_target_reports_the_attachment_for_an_invalid_name() {
        let id = Uuid::from_u128(2);

        let error = parse_target(id, "Not A Name!".into()).unwrap_err();

        assert!(matches!(
            error,
            RefreshStoreError::CorruptTarget { attachment_id } if attachment_id == id
        ));
    }
}
