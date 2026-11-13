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
/// Caches locator → `study_id` mappings and full study configs with a TTL.
pub struct RedisStudyCache {
    pool: Arc<RedisPool>,
}

const STUDY_CACHE_TTL_SECS: u64 = 300; // 5 minutes

impl RedisStudyCache {
    /// Creates a study cache that shares `pool` for all Redis access.
    pub fn new(pool: Arc<RedisPool>) -> Self {
        Self { pool }
    }

    /// Caches a locator → `study_id` mapping with no TTL; it persists until overwritten.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Connection`] if a Redis connection cannot be
    /// acquired, or [`RepositoryError::Storage`] if the write fails.
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
            .map_err(RepositoryError::connection)?;

        let key = format!("locator:{locator}");
        conn.set::<_, _, ()>(&key, study_id.to_string())
            .await
            .map_err(RepositoryError::storage)?;
        Ok(())
    }

    /// Looks up the cached `study_id` for a locator.
    ///
    /// Returns `None` if the locator is not cached.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Connection`] if a Redis connection cannot be
    /// acquired, or [`RepositoryError::Storage`] if the read fails or the
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
            .map_err(RepositoryError::connection)?;

        let key = format!("locator:{locator}");
        let val: Option<String> = conn.get(&key).await.map_err(RepositoryError::storage)?;

        match val {
            Some(s) => {
                let id = s
                    .parse::<Uuid>()
                    .map_err(|e| RepositoryError::corrupt("cached study ID", e))?;
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    /// Caches a full study config with a TTL.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Connection`] if a Redis connection cannot be
    /// acquired, or [`RepositoryError::Storage`] if serialization or the write
    /// fails.
    #[instrument(skip(self, study), fields(study_id = %study.id))]
    pub async fn cache_study(&self, study: &Study) -> Result<(), RepositoryError> {
        let mut conn = self
            .pool
            .connection()
            .await
            .map_err(RepositoryError::connection)?;

        let key = format!("study_cache:{}", study.id);
        let json = serde_json::to_string(study).map_err(RepositoryError::storage)?;

        conn.set_ex::<_, _, ()>(&key, &json, STUDY_CACHE_TTL_SECS)
            .await
            .map_err(RepositoryError::storage)?;
        Ok(())
    }

    /// Retrieves a cached study config.
    ///
    /// Returns `None` if the study is not cached or its entry has expired.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Connection`] if a Redis connection cannot be
    /// acquired, or [`RepositoryError::Storage`] if the read or
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
            .map_err(RepositoryError::connection)?;

        let key = format!("study_cache:{study_id}");
        let val: Option<String> = conn.get(&key).await.map_err(RepositoryError::storage)?;

        match val {
            Some(json) => {
                let study: Study = serde_json::from_str(&json)
                    .map_err(|e| RepositoryError::corrupt("cached study", e))?;
                Ok(Some(study))
            }
            None => Ok(None),
        }
    }

    /// Removes a study from the cache on update or delete.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Connection`] if a Redis connection cannot be
    /// acquired, or [`RepositoryError::Storage`] if the delete fails.
    #[instrument(skip(self))]
    pub async fn invalidate_study(&self, study_id: &Uuid) -> Result<(), RepositoryError> {
        let mut conn = self
            .pool
            .connection()
            .await
            .map_err(RepositoryError::connection)?;

        let key = format!("study_cache:{study_id}");
        conn.del::<_, ()>(&key)
            .await
            .map_err(RepositoryError::storage)?;
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
