use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use pocr_core::ports::{LocatorRepository, RepositoryError};

/// PostgreSQL-backed locator → study mapping repository.
pub struct PgLocatorRepository {
    pool: PgPool,
}

impl PgLocatorRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LocatorRepository for PgLocatorRepository {
    #[instrument(skip(self))]
    async fn save_mapping(&self, locator: u64, study_id: &Uuid) -> Result<(), RepositoryError> {
        let locator = locator_to_i64(locator)?;
        sqlx::query(
            r#"INSERT INTO locator_mappings (locator, study_id)
               VALUES ($1, $2)
               ON CONFLICT (locator) DO UPDATE SET study_id = EXCLUDED.study_id"#,
        )
        .bind(locator)
        .bind(study_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn find_study_id(&self, locator: u64) -> Result<Option<Uuid>, RepositoryError> {
        let locator = locator_to_i64(locator)?;
        let row: Option<(Uuid,)> =
            sqlx::query_as("SELECT study_id FROM locator_mappings WHERE locator = $1")
                .bind(locator)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| RepositoryError::Internal(e.to_string()))?;
        Ok(row.map(|(id,)| id))
    }
}

/// Convert an Epic locator to the `i64` PostgreSQL column type.
///
/// # Errors
///
/// Return [`RepositoryError::Internal`] if `locator` exceeds [`i64::MAX`].
fn locator_to_i64(locator: u64) -> Result<i64, RepositoryError> {
    i64::try_from(locator)
        .map_err(|_| RepositoryError::Internal(format!("locator {locator} exceeds i64")))
}
