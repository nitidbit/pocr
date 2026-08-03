use std::sync::Arc;

use async_trait::async_trait;
use redis::AsyncCommands;
use tracing::instrument;
use uuid::Uuid;

use pocr_core::ports::{RepositoryError, StudyCache};
use pocr_core::study::Study;

use crate::connection::RedisPool;

/// Redis cache for study configs on the CDS Hooks hot path.
///
/// Caches locator → study_id mappings and full study configs with TTL.
pub struct RedisStudyCache {
    pool: Arc<RedisPool>,
}

const STUDY_CACHE_TTL_SECS: u64 = 300; // 5 minutes

impl RedisStudyCache {
    pub fn new(pool: Arc<RedisPool>) -> Self {
        Self { pool }
    }

    /// Cache a locator → study_id mapping (no TTL — persists until overwritten).
    ///
    /// # Errors
    ///
    /// Return [`RepositoryError::Connection`] if a Redis connection cannot be
    /// acquired, or [`RepositoryError::Internal`] if the write fails.
    #[instrument(skip(self))]
    pub async fn cache_locator_mapping(
        &self,
        locator: u64,
        study_id: &Uuid,
    ) -> Result<(), RepositoryError> {
        let mut conn = self
            .pool
            .connection()
            .await
            .map_err(|e| RepositoryError::Connection(e.to_string()))?;

        let key = format!("locator:{locator}");
        conn.set::<_, _, ()>(&key, study_id.to_string())
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;
        Ok(())
    }

    /// Look up a cached study_id for a locator.
    ///
    /// Return `None` if the locator is not cached.
    ///
    /// # Errors
    ///
    /// Return [`RepositoryError::Connection`] if a Redis connection cannot be
    /// acquired, or [`RepositoryError::Internal`] if the read fails or the
    /// cached value is not a valid UUID.
    #[instrument(skip(self))]
    pub async fn get_study_id_for_locator(
        &self,
        locator: u64,
    ) -> Result<Option<Uuid>, RepositoryError> {
        let mut conn = self
            .pool
            .connection()
            .await
            .map_err(|e| RepositoryError::Connection(e.to_string()))?;

        let key = format!("locator:{locator}");
        let val: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        match val {
            Some(s) => {
                let id = s
                    .parse::<Uuid>()
                    .map_err(|e| RepositoryError::Internal(e.to_string()))?;
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    /// Cache a full study config with TTL.
    ///
    /// # Errors
    ///
    /// Return [`RepositoryError::Connection`] if a Redis connection cannot be
    /// acquired, or [`RepositoryError::Internal`] if serialization or the write
    /// fails.
    #[instrument(skip(self, study), fields(study_id = %study.id))]
    pub async fn cache_study(&self, study: &Study) -> Result<(), RepositoryError> {
        let mut conn = self
            .pool
            .connection()
            .await
            .map_err(|e| RepositoryError::Connection(e.to_string()))?;

        let key = format!("study_cache:{}", study.id);
        let json =
            serde_json::to_string(study).map_err(|e| RepositoryError::Internal(e.to_string()))?;

        conn.set_ex::<_, _, ()>(&key, &json, STUDY_CACHE_TTL_SECS)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;
        Ok(())
    }

    /// Retrieve a cached study config.
    ///
    /// Return `None` if the study is not cached or its entry has expired.
    ///
    /// # Errors
    ///
    /// Return [`RepositoryError::Connection`] if a Redis connection cannot be
    /// acquired, or [`RepositoryError::Internal`] if the read or
    /// deserialization fails.
    #[instrument(skip(self))]
    pub async fn get_cached_study(
        &self,
        study_id: &Uuid,
    ) -> Result<Option<Study>, RepositoryError> {
        let mut conn = self
            .pool
            .connection()
            .await
            .map_err(|e| RepositoryError::Connection(e.to_string()))?;

        let key = format!("study_cache:{study_id}");
        let val: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        match val {
            Some(json) => {
                let study: Study = serde_json::from_str(&json)
                    .map_err(|e| RepositoryError::Internal(e.to_string()))?;
                Ok(Some(study))
            }
            None => Ok(None),
        }
    }

    /// Remove a study from cache (used on update/delete).
    ///
    /// # Errors
    ///
    /// Return [`RepositoryError::Connection`] if a Redis connection cannot be
    /// acquired, or [`RepositoryError::Internal`] if the delete fails.
    #[instrument(skip(self))]
    pub async fn invalidate_study(&self, study_id: &Uuid) -> Result<(), RepositoryError> {
        let mut conn = self
            .pool
            .connection()
            .await
            .map_err(|e| RepositoryError::Connection(e.to_string()))?;

        let key = format!("study_cache:{study_id}");
        conn.del::<_, ()>(&key)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl StudyCache for RedisStudyCache {
    async fn get_study_id_for_locator(
        &self,
        locator: u64,
    ) -> Result<Option<Uuid>, RepositoryError> {
        RedisStudyCache::get_study_id_for_locator(self, locator).await
    }

    async fn cache_locator_mapping(
        &self,
        locator: u64,
        study_id: &Uuid,
    ) -> Result<(), RepositoryError> {
        RedisStudyCache::cache_locator_mapping(self, locator, study_id).await
    }

    async fn get_cached_study(&self, study_id: &Uuid) -> Result<Option<Study>, RepositoryError> {
        RedisStudyCache::get_cached_study(self, study_id).await
    }

    async fn cache_study(&self, study: &Study) -> Result<(), RepositoryError> {
        RedisStudyCache::cache_study(self, study).await
    }

    async fn invalidate_study(&self, study_id: &Uuid) -> Result<(), RepositoryError> {
        RedisStudyCache::invalidate_study(self, study_id).await
    }
}
