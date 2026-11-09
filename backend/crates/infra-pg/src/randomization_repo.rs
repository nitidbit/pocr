use crate::error::PgError;
use async_trait::async_trait;
use sqlx::PgPool;
use sqlx::types::Json;
use std::collections::BTreeMap;
use tracing::instrument;
use uuid::Uuid;

use pocr_core::environment::Environment;
use pocr_core::ports::{ArmTally, RandomizationRepository, RepositoryError};
use pocr_core::random::Randomization;

/// PostgreSQL-backed randomization record repository.
pub struct PgRandomizationRepository {
    pool: PgPool,
}

impl PgRandomizationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Columns selected for reconstructing a [`Randomization`].
///
/// `strata_key` is intentionally omitted: it is a lossy delimiter-joined
/// string, not the full [`Strata`](pocr_core::random::Strata) object, and is
/// never rehydrated on read.
const RANDOMIZATION_COLUMNS: &str =
    "id, study_id, environment, unit_hash, arm, issued_at,
     COALESCE((SELECT jsonb_object_agg(c.context_key, c.context_value)
               FROM randomization_context c WHERE c.randomization_id = randomizations.id), '{}') AS context";

#[derive(sqlx::FromRow)]
struct RandomizationRow {
    id: Uuid,
    study_id: Uuid,
    environment: String,
    unit_hash: String,
    arm: serde_json::Value,
    issued_at: chrono::DateTime<chrono::Utc>,
    context: Json<BTreeMap<String, serde_json::Value>>,
}

#[async_trait]
impl RandomizationRepository for PgRandomizationRepository {
    #[instrument(skip(self, randomization), fields(study_id = %randomization.study_id))]
    async fn save(&self, randomization: &Randomization) -> Result<Randomization, RepositoryError> {
        let unit_hash = String::from(&randomization.unit);
        let strata_key = randomization.strata.as_ref().map(String::from);
        let arm_json = serde_json::to_value(&randomization.arm)
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        let mut tx = self.pool.begin().await.map_err(PgError::from)?;
        sqlx::query(
            r#"INSERT INTO randomizations (id, study_id, environment, unit_hash, strata_key, arm, issued_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(randomization.id)
        .bind(randomization.study_id)
        .bind(randomization.environment.as_str())
        .bind(&unit_hash)
        .bind(&strata_key)
        .bind(&arm_json)
        .bind(randomization.issued_at)
        .execute(&mut *tx)
        .await
        .map_err(PgError::from)?;
        sqlx::query("INSERT INTO randomization_context (randomization_id, context_key, context_value) SELECT $1, key, value FROM jsonb_each($2)")
            .bind(randomization.id)
            .bind(Json(&randomization.context))
            .execute(&mut *tx).await.map_err(PgError::from)?;
        tx.commit().await.map_err(PgError::from)?;
        Ok(randomization.clone())
    }

    #[instrument(skip(self))]
    async fn find_by_study(
        &self,
        study_id: &Uuid,
        environment: Environment,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Randomization>, RepositoryError> {
        let query = format!(
            "SELECT {RANDOMIZATION_COLUMNS} FROM randomizations \
             WHERE study_id = $1 AND environment = $2 \
             ORDER BY issued_at DESC LIMIT $3 OFFSET $4"
        );
        let rows = sqlx::query_as::<_, RandomizationRow>(&query)
            .bind(study_id)
            .bind(environment.as_str())
            .bind(i64::from(limit))
            .bind(i64::from(offset))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        rows.into_iter().map(row_to_randomization).collect()
    }

    #[instrument(skip(self))]
    async fn count_by_arm(
        &self,
        study_id: &Uuid,
        environment: Environment,
    ) -> Result<Vec<ArmTally>, RepositoryError> {
        // Group on the arm's `id` field inside the JSONB column rather than the
        // whole blob: an arm's name/ratio/locator can change over time, so two
        // records for the same arm may carry different JSON.
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            "SELECT (arm->>'id')::uuid AS arm_id, COUNT(*) AS count \
             FROM randomizations \
             WHERE study_id = $1 AND environment = $2 \
             GROUP BY arm->>'id'",
        )
        .bind(study_id)
        .bind(environment.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(arm_id, count)| ArmTally {
                arm_id,
                count: count.max(0) as u64,
            })
            .collect())
    }

    #[instrument(skip(self))]
    async fn exists_for_unit(
        &self,
        study_id: &Uuid,
        environment: Environment,
        unit_key: &str,
    ) -> Result<Option<Randomization>, RepositoryError> {
        let query = format!(
            "SELECT {RANDOMIZATION_COLUMNS} FROM randomizations \
             WHERE study_id = $1 AND environment = $2 AND unit_hash = $3"
        );
        let row = sqlx::query_as::<_, RandomizationRow>(&query)
            .bind(study_id)
            .bind(environment.as_str())
            .bind(unit_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        row.map(row_to_randomization).transpose()
    }

    #[instrument(skip(self))]
    async fn purge_nonproduction(&self, study_id: &Uuid) -> Result<u64, RepositoryError> {
        let result =
            sqlx::query("DELETE FROM randomizations WHERE study_id = $1 AND environment = $2")
                .bind(study_id)
                .bind(Environment::NonProduction.as_str())
                .execute(&self.pool)
                .await
                .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        Ok(result.rows_affected())
    }
}

fn row_to_randomization(row: RandomizationRow) -> Result<Randomization, RepositoryError> {
    let arm = serde_json::from_value(row.arm)
        .map_err(|e| RepositoryError::Internal(format!("invalid arm JSON: {e}")))?;

    let environment = Environment::parse(&row.environment).ok_or_else(|| {
        RepositoryError::Internal(format!("invalid environment value: {}", row.environment))
    })?;

    // Lossy reconstruction: the persisted row keeps only the delimiter-joined
    // unit_key and strata_key, not the full Unit/Strata objects. The original
    // shape is unrecoverable, so unit_key is surfaced as a PatientId-wrapped
    // string and strata is dropped. Callers needing the true unit must not
    // rely on this read.
    use pocr_core::random::{SimpleUnit, Unit};
    let unit = Unit::Simple(SimpleUnit::PatientId(row.unit_hash.clone()));
    let strata = None;

    Ok(Randomization {
        id: row.id,
        study_id: row.study_id,
        environment,
        strata,
        unit,
        arm,
        issued_at: row.issued_at,
        context: row.context.0,
    })
}
