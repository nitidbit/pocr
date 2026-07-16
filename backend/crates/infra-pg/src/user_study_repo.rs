use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use pocr_core::ports::{RepositoryError, UserStudyRepository};

/// PostgreSQL-backed user ↔ study association repository.
pub struct PgUserStudyRepository {
    pool: PgPool,
}

impl PgUserStudyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserStudyRepository for PgUserStudyRepository {
    #[instrument(skip(self))]
    async fn add_user(
        &self,
        user_id: &str,
        study_id: &Uuid,
        role: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"INSERT INTO user_studies (user_id, study_id, role)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id, study_id) DO NOTHING"#,
        )
        .bind(user_id)
        .bind(study_id)
        .bind(role)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn remove_user(&self, user_id: &str, study_id: &Uuid) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM user_studies WHERE user_id = $1 AND study_id = $2")
            .bind(user_id)
            .bind(study_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;
        Ok(())
    }
}
