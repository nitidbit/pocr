use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use pocr_core::environment::Environment;
use pocr_core::ports::{RandomizationRepository, RepositoryError};
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

#[derive(sqlx::FromRow)]
struct RandomizationRow {
    id: Uuid,
    study_id: Uuid,
    environment: String,
    unit_hash: String,
    #[allow(dead_code)]
    strata_key: Option<String>,
    arm: serde_json::Value,
    issued_at: chrono::DateTime<chrono::Utc>,
    epic_patient_id: Option<String>,
    epic_csn: Option<String>,
    epic_user_id: Option<String>,
}

#[async_trait]
impl RandomizationRepository for PgRandomizationRepository {
    #[instrument(skip(self, randomization), fields(study_id = %randomization.study_id))]
    async fn save(&self, randomization: &Randomization) -> Result<Randomization, RepositoryError> {
        let unit_hash = String::from(&randomization.unit);
        let strata_key = randomization.strata.as_ref().map(String::from);
        let arm_json = serde_json::to_value(&randomization.arm)
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        sqlx::query(
            r#"INSERT INTO randomizations (id, study_id, environment, unit_hash, strata_key, arm, issued_at, epic_patient_id, epic_csn, epic_user_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(randomization.id)
        .bind(randomization.study_id)
        .bind(randomization.environment.as_str())
        .bind(&unit_hash)
        .bind(&strata_key)
        .bind(&arm_json)
        .bind(randomization.issued_at)
        .bind(&randomization.epic_patient_id)
        .bind(&randomization.epic_csn)
        .bind(&randomization.epic_user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;

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
        let rows = sqlx::query_as::<_, RandomizationRow>(
            r#"SELECT * FROM randomizations WHERE study_id = $1 AND environment = $2
               ORDER BY issued_at DESC LIMIT $3 OFFSET $4"#,
        )
        .bind(study_id)
        .bind(environment.as_str())
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        rows.into_iter().map(row_to_randomization).collect()
    }

    #[instrument(skip(self))]
    async fn exists_for_unit(
        &self,
        study_id: &Uuid,
        environment: Environment,
        unit_hash: &str,
    ) -> Result<Option<Randomization>, RepositoryError> {
        let row = sqlx::query_as::<_, RandomizationRow>(
            "SELECT * FROM randomizations WHERE study_id = $1 AND environment = $2 AND unit_hash = $3",
        )
        .bind(study_id)
        .bind(environment.as_str())
        .bind(unit_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        row.map(row_to_randomization).transpose()
    }

    #[instrument(skip(self))]
    async fn purge_nonproduction(&self, study_id: &Uuid) -> Result<u64, RepositoryError> {
        let result = sqlx::query(
            "DELETE FROM randomizations WHERE study_id = $1 AND environment = $2",
        )
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

    // Reconstruct a minimal Randomization — the unit is stored as hash only,
    // so we use a placeholder. The full unit is in the arm context.
    use pocr_core::random::{SimpleUnit, Unit};
    let unit = Unit::Simple(SimpleUnit::PatientId(row.unit_hash.clone()));
    let strata = None; // Strata stored as key string, not full object

    Ok(Randomization {
        id: row.id,
        study_id: row.study_id,
        environment,
        strata,
        unit,
        arm,
        issued_at: row.issued_at,
        epic_patient_id: row.epic_patient_id,
        epic_csn: row.epic_csn,
        epic_user_id: row.epic_user_id,
    })
}
