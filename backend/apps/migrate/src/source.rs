//! Legacy Redis reader.
//!
//! Scans the legacy engine's keyspace and deserializes each record's JSON
//! string into the [`crate::legacy`] source models. All reads are non-mutating;
//! the migration never writes back to the legacy store.

use redis::AsyncCommands;
use tracing::{debug, instrument};

use infra_redis::RedisPool;

use crate::legacy::{LegacyBlock, LegacyLocatorMapping, LegacyRandomization, LegacyStudy};

/// Errors reading the legacy Redis store.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// A Redis command failed.
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    /// A stored JSON value could not be parsed into a source model.
    #[error("failed to parse {key}: {source}")]
    Parse {
        /// The Redis key whose value failed to parse.
        key: String,
        /// The underlying serde error.
        source: serde_json::Error,
    },
}

/// A locator record paired with its numeric locator id (parsed from the key).
#[derive(Debug, Clone)]
pub struct LocatorRecord {
    /// The Epic locator (LGL) id, from the `locator:{id}` key.
    pub locator: u64,
    /// The study the locator resolves to.
    pub mapping: LegacyLocatorMapping,
}

/// Reads legacy records from a Redis connection pool.
pub struct LegacySource {
    pool: RedisPool,
}

impl LegacySource {
    /// Wrap a Redis pool connected to the legacy store.
    pub fn new(pool: RedisPool) -> Self {
        Self { pool }
    }

    /// All `study:*` records.
    ///
    /// # Errors
    ///
    /// Return [`SourceError::Redis`] if a SCAN or GET fails, or
    /// [`SourceError::Parse`] if a stored value is not a valid `LegacyStudy`.
    #[instrument(skip(self))]
    pub async fn studies(&self) -> Result<Vec<LegacyStudy>, SourceError> {
        self.load_prefix("study:").await
    }

    /// All `randomization:*` records.
    ///
    /// # Errors
    ///
    /// Return [`SourceError::Redis`] if a SCAN or GET fails, or
    /// [`SourceError::Parse`] if a stored value is not a valid
    /// `LegacyRandomization`.
    #[instrument(skip(self))]
    pub async fn randomizations(&self) -> Result<Vec<LegacyRandomization>, SourceError> {
        self.load_prefix("randomization:").await
    }

    /// All `block:*` records.
    ///
    /// # Errors
    ///
    /// Return [`SourceError::Redis`] if a SCAN or GET fails, or
    /// [`SourceError::Parse`] if a stored value is not a valid `LegacyBlock`.
    #[instrument(skip(self))]
    pub async fn blocks(&self) -> Result<Vec<LegacyBlock>, SourceError> {
        self.load_prefix("block:").await
    }

    /// All `locator:*` records, each paired with its parsed locator id.
    ///
    /// Keys whose suffix is not a valid `u64` are skipped with a warning so a
    /// single malformed key cannot abort the migration.
    ///
    /// # Errors
    ///
    /// Return [`SourceError::Redis`] if a SCAN or GET fails, or
    /// [`SourceError::Parse`] if a stored value is not a valid
    /// `LegacyLocatorMapping`.
    #[instrument(skip(self))]
    pub async fn locators(&self) -> Result<Vec<LocatorRecord>, SourceError> {
        let keys = self.scan_keys("locator:*").await?;
        let mut records = Vec::with_capacity(keys.len());
        for key in keys {
            let Some(suffix) = key.strip_prefix("locator:") else {
                continue;
            };
            let Ok(locator) = suffix.parse::<u64>() else {
                tracing::warn!(key, "skipping locator key with non-numeric id");
                continue;
            };
            let mapping = self.load_one(&key).await?;
            records.push(LocatorRecord { locator, mapping });
        }
        Ok(records)
    }

    /// Load and parse every value under keys matching `{prefix}*`.
    async fn load_prefix<T>(&self, prefix: &str) -> Result<Vec<T>, SourceError>
    where
        T: serde::de::DeserializeOwned,
    {
        let keys = self.scan_keys(&format!("{prefix}*")).await?;
        debug!(prefix, count = keys.len(), "scanned legacy keys");
        let mut records = Vec::with_capacity(keys.len());
        for key in keys {
            records.push(self.load_one(&key).await?);
        }
        Ok(records)
    }

    /// Load and parse a single key's JSON string value.
    async fn load_one<T>(&self, key: &str) -> Result<T, SourceError>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut conn = self.pool.connection().await?;
        let raw: String = conn.get(key).await?;
        serde_json::from_str(&raw).map_err(|source| SourceError::Parse {
            key: key.to_string(),
            source,
        })
    }

    /// SCAN the keyspace for keys matching `pattern`, accumulating all matches.
    ///
    /// SCAN is used rather than KEYS so a large legacy keyspace does not block
    /// the Redis server during the migration.
    async fn scan_keys(&self, pattern: &str) -> Result<Vec<String>, SourceError> {
        let mut conn = self.pool.connection().await?;
        let mut cursor: u64 = 0;
        let mut keys = Vec::new();
        loop {
            let (next, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(500)
                .query_async(&mut conn)
                .await?;
            keys.extend(batch);
            cursor = next;
            if cursor == 0 {
                break;
            }
        }
        Ok(keys)
    }
}
