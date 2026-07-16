//! POCR write side: PostgreSQL rows and Redis block state.
//!
//! Studies, locator mappings, and user↔study rows are written through the
//! existing `infra-pg` repositories, whose `save`/`save_mapping`/`add_user`
//! are already idempotent upserts. Randomization records are written here with
//! a direct upsert because the repository's `save` is a plain `INSERT`: the
//! migration must upsert on the environment-partitioned uniqueness key
//! `(study_id, environment, unit_hash)` so a re-run does not fail on conflict.
//! Blocks are written via `RedisBlockCache`, which prepends the `production`
//! environment segment to the key.

use sqlx::PgPool;
use tracing::instrument;

use infra_pg::{PgLocatorRepository, PgStudyRepository, PgUserStudyRepository};
use infra_redis::{RedisBlockCache, RedisPool};
use pocr_core::environment::Environment;
use pocr_core::ports::{
    BlockCacheRepository, LocatorRepository, StudyRepository, UserStudyRepository,
};
use pocr_core::random::{RandomPermutedBlock, Randomization};
use pocr_core::study::Study;

use std::sync::Arc;
use uuid::Uuid;

/// Errors writing to the POCR stores.
#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    /// A repository operation failed.
    #[error("repository error: {0}")]
    Repository(#[from] pocr_core::ports::RepositoryError),
    /// A direct PostgreSQL query failed.
    #[error("postgres error: {0}")]
    Postgres(#[from] sqlx::Error),
    /// Serializing a record to JSONB failed.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Writes migrated records to PostgreSQL and Redis.
pub struct PocrSink {
    pool: PgPool,
    studies: PgStudyRepository,
    locators: PgLocatorRepository,
    users: PgUserStudyRepository,
    blocks: RedisBlockCache,
}

impl PocrSink {
    /// Build a sink over a Postgres pool and a Redis pool.
    pub fn new(pool: PgPool, redis: Arc<RedisPool>) -> Self {
        Self {
            studies: PgStudyRepository::new(pool.clone()),
            locators: PgLocatorRepository::new(pool.clone()),
            users: PgUserStudyRepository::new(pool.clone()),
            blocks: RedisBlockCache::new(redis),
            pool,
        }
    }

    /// Upsert a study (idempotent on the study PK).
    #[instrument(skip(self, study), fields(study_id = %study.id))]
    pub async fn write_study(&self, study: &Study) -> Result<(), SinkError> {
        self.studies.save(study).await?;
        Ok(())
    }

    /// Upsert a randomization record on `(study_id, environment, unit_hash)`.
    ///
    /// Mirrors the `infra-pg` row mapping — `unit_hash`/`strata_key` are the
    /// shared `String::from` hashes — but adds the `ON CONFLICT` clause the
    /// migration needs for idempotent re-runs. On conflict the arm, issued-at,
    /// and Epic-id columns are refreshed so a corrected re-run converges.
    #[instrument(skip(self, rec), fields(study_id = %rec.study_id))]
    pub async fn write_randomization(&self, rec: &Randomization) -> Result<(), SinkError> {
        let unit_hash = String::from(&rec.unit);
        let strata_key = rec.strata.as_ref().map(String::from);
        let arm_json = serde_json::to_value(&rec.arm)?;

        sqlx::query(
            r#"INSERT INTO randomizations
                 (id, study_id, environment, unit_hash, strata_key, arm, issued_at,
                  epic_patient_id, csn, epic_provider_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
               ON CONFLICT (study_id, environment, unit_hash) DO UPDATE SET
                 arm = EXCLUDED.arm,
                 issued_at = EXCLUDED.issued_at,
                 epic_patient_id = EXCLUDED.epic_patient_id,
                 csn = EXCLUDED.csn,
                 epic_provider_id = EXCLUDED.epic_provider_id"#,
        )
        .bind(rec.id)
        .bind(rec.study_id)
        .bind(rec.environment.as_str())
        .bind(&unit_hash)
        .bind(&strata_key)
        .bind(&arm_json)
        .bind(rec.issued_at)
        .bind(&rec.epic_patient_id)
        .bind(&rec.csn)
        .bind(&rec.epic_provider_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Upsert a locator → study mapping.
    #[instrument(skip(self))]
    pub async fn write_locator(&self, locator: u64, study_id: &Uuid) -> Result<(), SinkError> {
        self.locators.save_mapping(locator, study_id).await?;
        Ok(())
    }

    /// Associate a user with a study as `owner` (idempotent).
    #[instrument(skip(self))]
    pub async fn write_owner(&self, user_id: &str, study_id: &Uuid) -> Result<(), SinkError> {
        self.users.add_user(user_id, study_id, "owner").await?;
        Ok(())
    }

    /// Write a production block into the Redis cache.
    ///
    /// `RedisBlockCache::save_block` keys it `block:production:{study_id}:{stratum_key}`.
    #[instrument(skip(self, block), fields(study_id = %block.study_id))]
    pub async fn write_block(&self, block: &RandomPermutedBlock) -> Result<(), SinkError> {
        self.blocks
            .save_block(Environment::Production, block)
            .await?;
        Ok(())
    }
}
