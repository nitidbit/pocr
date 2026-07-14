//! Repository port traits — infrastructure-agnostic persistence interfaces.
//!
//! These traits define the boundary between domain logic and storage.
//! Implementations live in `infra-pg` and `infra-redis`.

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::error::RandomizationError;
use crate::random::{Randomization, RandomPermutedBlock, RandomizationRequest};
use crate::study::Study;

/// Errors that can occur in repository operations.
#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("connection error: {0}")]
    Connection(String),
    #[error("internal error: {0}")]
    Internal(String),
}

/// Persistence for study aggregates.
#[async_trait]
pub trait StudyRepository: Send + Sync {
    /// Insert or update a study.
    async fn save(&self, study: &Study) -> Result<Study, RepositoryError>;
    /// Find a study by its ID.
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Study>, RepositoryError>;
    /// Find a study by its Epic locator ID.
    async fn find_by_locator(&self, locator: u64) -> Result<Option<Study>, RepositoryError>;
    /// List all studies accessible to a user.
    async fn list_for_user(&self, user_id: &str) -> Result<Vec<Study>, RepositoryError>;
    /// Delete a study by ID.
    async fn delete(&self, id: &Uuid) -> Result<(), RepositoryError>;
}

/// Persistence for randomization assignment records.
#[async_trait]
pub trait RandomizationRepository: Send + Sync {
    /// Store a new randomization record.
    async fn save(&self, randomization: &Randomization) -> Result<Randomization, RepositoryError>;
    /// List randomizations for a study with pagination.
    async fn find_by_study(
        &self,
        study_id: &Uuid,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Randomization>, RepositoryError>;
    /// Check if a randomization already exists for this study + unit combination.
    /// Returns the existing record if found.
    async fn exists_for_unit(
        &self,
        study_id: &Uuid,
        unit_hash: &str,
    ) -> Result<Option<Randomization>, RepositoryError>;
}

/// Cache for permuted blocks (Redis hot path).
#[async_trait]
pub trait BlockCacheRepository: Send + Sync {
    /// Retrieve the current block for a study/stratum combination.
    async fn get_block(
        &self,
        study_id: &Uuid,
        stratum_key: &str,
    ) -> Result<Option<RandomPermutedBlock>, RepositoryError>;
    /// Store a block in the cache.
    async fn save_block(&self, block: &RandomPermutedBlock) -> Result<(), RepositoryError>;
    /// Atomically consume the next assignment from a cached block.
    /// Returns `None` if the block is exhausted or not found.
    async fn consume_next(
        &self,
        study_id: &Uuid,
        stratum_key: &str,
    ) -> Result<Option<u32>, RepositoryError>;
}

/// Persistence for Epic locator → study mappings.
#[async_trait]
pub trait LocatorRepository: Send + Sync {
    /// Create or update a locator → study mapping.
    async fn save_mapping(&self, locator: u64, study_id: &Uuid) -> Result<(), RepositoryError>;
    /// Look up which study a locator belongs to.
    async fn find_study_id(&self, locator: u64) -> Result<Option<Uuid>, RepositoryError>;
}

/// Persistence for user ↔ study associations.
#[async_trait]
pub trait UserStudyRepository: Send + Sync {
    /// Associate a user with a study in the given role.
    async fn add_user(
        &self,
        user_id: &str,
        study_id: &Uuid,
        role: &str,
    ) -> Result<(), RepositoryError>;
    /// Remove a user's association with a study.
    async fn remove_user(&self, user_id: &str, study_id: &Uuid) -> Result<(), RepositoryError>;
}

/// Cache for study configs on the CDS Hooks hot path.
#[async_trait]
pub trait StudyCache: Send + Sync {
    /// Look up cached study_id for a locator.
    async fn get_study_id_for_locator(&self, locator: u64) -> Result<Option<Uuid>, RepositoryError>;
    /// Cache a locator → study_id mapping.
    async fn cache_locator_mapping(&self, locator: u64, study_id: &Uuid) -> Result<(), RepositoryError>;
    /// Retrieve a cached study config.
    async fn get_cached_study(&self, study_id: &Uuid) -> Result<Option<Study>, RepositoryError>;
    /// Cache a full study config with TTL.
    async fn cache_study(&self, study: &Study) -> Result<(), RepositoryError>;
    /// Remove a study from cache.
    async fn invalidate_study(&self, study_id: &Uuid) -> Result<(), RepositoryError>;
}

/// Trait for checking if an infrastructure service is healthy.
#[async_trait]
pub trait HealthCheckable: Send + Sync {
    async fn ping(&self) -> Result<(), String>;
}

/// Orchestrates the randomization flow for a study + request.
#[async_trait]
pub trait RandomizationService: Send + Sync {
    async fn randomize(
        &self,
        study: &Study,
        request: &RandomizationRequest,
    ) -> Result<Randomization, RandomizationError>;
}
