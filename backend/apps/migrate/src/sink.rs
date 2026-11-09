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
    ///
    /// # Errors
    ///
    /// Return [`SinkError::Repository`] if the underlying study upsert fails.
    #[instrument(skip(self, study), fields(study_id = %study.id))]
    pub async fn write_study(&self, study: &Study) -> Result<(), SinkError> {
        self.studies.save(study).await?;
        Ok(())
    }

    /// Upsert a randomization record on `(study_id, environment, unit_hash)`.
    ///
    /// Mirrors the `infra-pg` row mapping — `unit_hash`/`strata_key` are the
    /// shared `String::from` hashes — but adds the `ON CONFLICT` clause the
    /// migration needs for idempotent re-runs. Existing assignments and context
    /// are immutable: a rerun never overwrites manually enriched identifiers.
    ///
    /// # Errors
    ///
    /// Return [`SinkError::Serialization`] if the arm cannot be encoded to
    /// JSONB, or [`SinkError::Postgres`] if the upsert fails.
    #[instrument(skip(self, rec), fields(study_id = %rec.study_id))]
    pub async fn write_randomization(&self, rec: &Randomization) -> Result<(), SinkError> {
        let unit_hash = String::from(&rec.unit);
        let strata_key = rec.strata.as_ref().map(String::from);
        let arm_json = serde_json::to_value(&rec.arm)?;

        let mut tx = self.pool.begin().await?;
        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"INSERT INTO randomizations
                 (id, study_id, environment, unit_hash, strata_key, arm, issued_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               ON CONFLICT (study_id, environment, unit_hash) DO NOTHING
               RETURNING id"#,
        )
        .bind(rec.id)
        .bind(rec.study_id)
        .bind(rec.environment.as_str())
        .bind(&unit_hash)
        .bind(&strata_key)
        .bind(&arm_json)
        .bind(rec.issued_at)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(id) = inserted {
            sqlx::query("INSERT INTO randomization_context (randomization_id, context_key, context_value) SELECT $1, key, value FROM jsonb_each($2)")
                .bind(id).bind(sqlx::types::Json(&rec.context)).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Upsert a locator → study mapping.
    ///
    /// # Errors
    ///
    /// Return [`SinkError::Repository`] if the mapping upsert fails.
    #[instrument(skip(self))]
    pub async fn write_locator(&self, locator: u64, study_id: &Uuid) -> Result<(), SinkError> {
        self.locators.save_mapping(locator, study_id).await?;
        Ok(())
    }

    /// Associate a user with a study as `owner` (idempotent).
    ///
    /// # Errors
    ///
    /// Return [`SinkError::Repository`] if the association upsert fails.
    #[instrument(skip(self))]
    pub async fn write_owner(&self, user_id: &str, study_id: &Uuid) -> Result<(), SinkError> {
        self.users.add_user(user_id, study_id, "owner").await?;
        Ok(())
    }

    /// Write a production block into the Redis cache.
    ///
    /// `RedisBlockCache::save_block` keys it `block:production:{study_id}:{stratum_key}`.
    ///
    /// # Errors
    ///
    /// Return [`SinkError::Repository`] if the Redis block write fails.
    #[instrument(skip(self, block), fields(study_id = %block.study_id))]
    pub async fn write_block(&self, block: &RandomPermutedBlock) -> Result<(), SinkError> {
        self.blocks
            .save_block(Environment::Production, block)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocr_core::random::{Arm, SimpleUnit, Unit};

    #[sqlx::test(migrations = "../../crates/infra-pg/migrations")]
    #[ignore = "requires disposable PostgreSQL via DATABASE_URL"]
    async fn import_preserves_existing_assignment_and_context_on_conflict(pool: PgPool) {
        let study_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO studies (id, name, randomization_config) VALUES ($1,'Synthetic','{}')",
        )
        .bind(study_id)
        .execute(&pool)
        .await
        .unwrap();
        let redis = Arc::new(RedisPool::new("redis://127.0.0.1:1").unwrap());
        let sink = PocrSink::new(pool.clone(), redis);
        let mut rec = Randomization::simple(
            &study_id,
            Environment::Production,
            &Unit::Simple(SimpleUnit::EncounterId("enc".into())),
            &Arm::new("A", 1),
        );
        rec.context.insert("fhir_encounter_id".into(), "enc".into());
        rec.context.insert("epic_csn".into(), "00123".into());
        sink.write_randomization(&rec).await.unwrap();
        let context: serde_json::Value = sqlx::query_scalar("SELECT COALESCE(jsonb_object_agg(context_key,context_value),'{}') FROM randomization_context WHERE randomization_id=$1").bind(rec.id).fetch_one(&pool).await.unwrap();
        assert_eq!(
            context,
            serde_json::json!({"fhir_encounter_id":"enc", "epic_csn":"00123"})
        );
        let original: serde_json::Value =
            sqlx::query_scalar("SELECT to_jsonb(r) FROM randomizations r WHERE id=$1")
                .bind(rec.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let original_id = rec.id;
        rec.id = Uuid::now_v7();
        rec.arm = Arm::new("different", 1);
        rec.context.remove("epic_csn");
        rec.context
            .insert("fhir_encounter_id".into(), "changed".into());
        sink.write_randomization(&rec).await.unwrap();
        let after: serde_json::Value =
            sqlx::query_scalar("SELECT to_jsonb(r) FROM randomizations r WHERE id=$1")
                .bind(original_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(original, after);
        let after_context: serde_json::Value = sqlx::query_scalar("SELECT jsonb_object_agg(context_key,context_value) FROM randomization_context WHERE randomization_id=$1").bind(original_id).fetch_one(&pool).await.unwrap();
        assert_eq!(context, after_context);
    }
}
