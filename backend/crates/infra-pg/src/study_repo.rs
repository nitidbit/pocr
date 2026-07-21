use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use pocr_core::ports::{RepositoryError, StudyRepository};
use pocr_core::study::Study;

/// PostgreSQL-backed study repository.
pub struct PgStudyRepository {
    pool: PgPool,
}

impl PgStudyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Intermediate row type mapping the database schema.
#[derive(sqlx::FromRow)]
struct StudyRow {
    id: Uuid,
    name: String,
    description: String,
    status: String,
    randomization_config: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<StudyRow> for Study {
    type Error = RepositoryError;

    fn try_from(row: StudyRow) -> Result<Self, Self::Error> {
        let status = serde_json::from_value(serde_json::Value::String(row.status))
            .map_err(|e| RepositoryError::Internal(format!("invalid status: {e}")))?;
        let randomization_config = serde_json::from_value(row.randomization_config)
            .map_err(|e| RepositoryError::Internal(format!("invalid config: {e}")))?;

        Ok(Study {
            id: row.id,
            name: row.name,
            description: row.description,
            status,
            randomization_config,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[async_trait]
impl StudyRepository for PgStudyRepository {
    #[instrument(skip(self, study), fields(study_id = %study.id))]
    async fn save(&self, study: &Study) -> Result<Study, RepositoryError> {
        let config_json = serde_json::to_value(&study.randomization_config)
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;
        let status_str = serde_json::to_value(study.status)
            .map_err(|e| RepositoryError::Internal(e.to_string()))?
            .as_str()
            .expect("StudyStatus serializes as a string")
            .to_string();

        let row = sqlx::query_as::<_, StudyRow>(
            r#"INSERT INTO studies (id, name, description, status, randomization_config, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               ON CONFLICT (id) DO UPDATE SET
                 name = EXCLUDED.name,
                 description = EXCLUDED.description,
                 status = EXCLUDED.status,
                 randomization_config = EXCLUDED.randomization_config,
                 updated_at = NOW()
               RETURNING *"#,
        )
        .bind(study.id)
        .bind(&study.name)
        .bind(&study.description)
        .bind(&status_str)
        .bind(&config_json)
        .bind(study.created_at)
        .bind(study.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        row.try_into()
    }

    #[instrument(skip(self))]
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Study>, RepositoryError> {
        let row = sqlx::query_as::<_, StudyRow>("SELECT * FROM studies WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        row.map(Study::try_from).transpose()
    }

    #[instrument(skip(self))]
    async fn find_by_locator(&self, locator: u64) -> Result<Option<Study>, RepositoryError> {
        let locator = i64::try_from(locator)
            .map_err(|_| RepositoryError::Internal(format!("locator {locator} exceeds i64")))?;
        let row = sqlx::query_as::<_, StudyRow>(
            r#"SELECT s.* FROM studies s
               JOIN locator_mappings lm ON lm.study_id = s.id
               WHERE lm.locator = $1"#,
        )
        .bind(locator)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        row.map(Study::try_from).transpose()
    }

    #[instrument(skip(self))]
    async fn list_for_user(&self, user_id: &str) -> Result<Vec<Study>, RepositoryError> {
        let rows = sqlx::query_as::<_, StudyRow>(
            r#"SELECT s.* FROM studies s
               JOIN user_studies us ON us.study_id = s.id
               WHERE us.user_id = $1
               ORDER BY s.updated_at DESC"#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        rows.into_iter().map(Study::try_from).collect()
    }

    #[instrument(skip(self))]
    async fn delete(&self, id: &Uuid) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM studies WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;
        Ok(())
    }
}
