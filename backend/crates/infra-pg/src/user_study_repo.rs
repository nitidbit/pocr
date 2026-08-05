use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use pocr_core::ports::{RepositoryError, StudyMember, UserStudyRepository};

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
               ON CONFLICT (user_id, study_id) DO UPDATE SET role = EXCLUDED.role"#,
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

    #[instrument(skip(self))]
    async fn get_role(
        &self,
        user_id: &str,
        study_id: &Uuid,
    ) -> Result<Option<String>, RepositoryError> {
        let role = sqlx::query_scalar::<_, String>(
            "SELECT role FROM user_studies WHERE user_id = $1 AND study_id = $2",
        )
        .bind(user_id)
        .bind(study_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;
        Ok(role)
    }

    #[instrument(skip(self))]
    async fn list_members(&self, study_id: &Uuid) -> Result<Vec<StudyMember>, RepositoryError> {
        let rows = sqlx::query_as::<_, MemberRow>(
            r#"SELECT user_id, role, joined_at
               FROM user_studies
               WHERE study_id = $1
               ORDER BY joined_at"#,
        )
        .bind(study_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(StudyMember::from).collect())
    }
}

/// Intermediate row type mapping a `user_studies` membership record.
#[derive(sqlx::FromRow)]
struct MemberRow {
    user_id: String,
    role: String,
    joined_at: chrono::DateTime<chrono::Utc>,
}

impl From<MemberRow> for StudyMember {
    fn from(row: MemberRow) -> Self {
        StudyMember {
            user_id: row.user_id,
            role: row.role,
            joined_at: row.joined_at,
        }
    }
}
