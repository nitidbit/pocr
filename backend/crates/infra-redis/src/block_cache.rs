use async_trait::async_trait;
use redis::AsyncCommands;
use tracing::instrument;
use uuid::Uuid;

use pocr_core::environment::Environment;
use pocr_core::ports::{BlockCacheRepository, RepositoryError};
use pocr_core::random::RandomPermutedBlock;

use std::sync::Arc;

use crate::connection::RedisPool;

/// Redis-backed permuted block cache.
///
/// Stores in-flight blocks keyed by `block:{env}:{study_id}:{stratum_key}`.
/// The environment prefix keeps production and non-production block sequences
/// fully independent. Uses a Lua script for atomic consume-next operations.
pub struct RedisBlockCache {
    pool: Arc<RedisPool>,
}

impl RedisBlockCache {
    pub fn new(pool: Arc<RedisPool>) -> Self {
        Self { pool }
    }

    fn block_key(environment: Environment, study_id: &Uuid, stratum_key: &str) -> String {
        format!("block:{}:{study_id}:{stratum_key}", environment.as_str())
    }
}

/// Lua script for atomically consuming the next value from a block.
/// Returns the arm index or nil if block is exhausted/missing.
const CONSUME_NEXT_SCRIPT: &str = r#"
local key = KEYS[1]
local data = redis.call('GET', key)
if not data then
    return nil
end
local block = cjson.decode(data)
local index = block['index']
local values = block['values']
if index >= #values then
    redis.call('DEL', key)
    return nil
end
block['index'] = index + 1
redis.call('SET', key, cjson.encode(block))
return values[index + 1]
"#;

#[async_trait]
impl BlockCacheRepository for RedisBlockCache {
    #[instrument(skip(self))]
    async fn get_block(
        &self,
        environment: Environment,
        study_id: &Uuid,
        stratum_key: &str,
    ) -> Result<Option<RandomPermutedBlock>, RepositoryError> {
        let mut conn = self
            .pool
            .connection()
            .await
            .map_err(|e| RepositoryError::Connection(e.to_string()))?;

        let key = Self::block_key(environment, study_id, stratum_key);
        let data: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        match data {
            Some(json) => {
                let block: RandomPermutedBlock = serde_json::from_str(&json)
                    .map_err(|e| RepositoryError::Internal(e.to_string()))?;
                Ok(Some(block))
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, block), fields(study_id = %block.study_id))]
    async fn save_block(
        &self,
        environment: Environment,
        block: &RandomPermutedBlock,
    ) -> Result<(), RepositoryError> {
        let mut conn = self
            .pool
            .connection()
            .await
            .map_err(|e| RepositoryError::Connection(e.to_string()))?;

        let stratum_key = block.stratification.as_deref().unwrap_or("");
        let key = Self::block_key(environment, &block.study_id, stratum_key);
        let json =
            serde_json::to_string(block).map_err(|e| RepositoryError::Internal(e.to_string()))?;

        conn.set::<_, _, ()>(&key, &json)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn consume_next(
        &self,
        environment: Environment,
        study_id: &Uuid,
        stratum_key: &str,
    ) -> Result<Option<u32>, RepositoryError> {
        let mut conn = self
            .pool
            .connection()
            .await
            .map_err(|e| RepositoryError::Connection(e.to_string()))?;

        let key = Self::block_key(environment, study_id, stratum_key);

        let result: Option<i64> = redis::Script::new(CONSUME_NEXT_SCRIPT)
            .key(&key)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        result
            .map(|v| {
                u32::try_from(v).map_err(|_| {
                    RepositoryError::Internal(format!("block arm index {v} out of u32 range"))
                })
            })
            .transpose()
    }

    #[instrument(skip(self))]
    async fn purge_nonproduction(&self, study_id: &Uuid) -> Result<u64, RepositoryError> {
        let mut conn = self
            .pool
            .connection()
            .await
            .map_err(|e| RepositoryError::Connection(e.to_string()))?;

        // Match every non-production block for this study across all strata.
        let pattern = format!("block:{}:{study_id}:*", Environment::NonProduction.as_str());

        // SCAN incrementally to avoid blocking Redis on large keyspaces.
        let mut cursor: u64 = 0;
        let mut removed: u64 = 0;
        loop {
            let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await
                .map_err(|e| RepositoryError::Internal(e.to_string()))?;

            if !keys.is_empty() {
                let deleted: u64 = conn
                    .del(&keys)
                    .await
                    .map_err(|e| RepositoryError::Internal(e.to_string()))?;
                removed += deleted;
            }

            cursor = next;
            if cursor == 0 {
                break;
            }
        }

        Ok(removed)
    }
}
