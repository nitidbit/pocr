//! Repository port traits — infrastructure-agnostic persistence interfaces.
//!
//! These traits define the boundary between domain logic and storage.
//! Implementations live in `infra-pg` and `infra-redis`.

use std::num::TryFromIntError;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::audit::AuditEvent;
use crate::demographics::{PatientDemographics, PatientDemographicsWithArm};
use crate::environment::Environment;
use crate::error::RandomizationError;
use crate::random::{RandomPermutedBlock, Randomization, RandomizationRequest};
use crate::study::Study;

/// Boxed cause carried by [`RepositoryError`], keeping `core` independent of
/// any driver crate.
type Cause = Box<dyn std::error::Error + Send + Sync>;

/// Errors that can occur in repository operations.
///
/// Driver errors are carried as the [`source`](std::error::Error::source) of
/// the variant that classifies them; their text is not part of the variant's
/// message.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RepositoryError {
    /// The requested record does not exist.
    #[error("not found")]
    NotFound,
    /// The write conflicts with existing state. The message is safe to show clients.
    #[error("conflict: {0}")]
    Conflict(String),
    /// A connection to the store could not be acquired.
    #[error("store connection failed")]
    Connection(#[source] Cause),
    /// A store operation failed after the connection was acquired.
    #[error("store operation failed")]
    Storage(#[source] Cause),
    /// A stored value cannot be decoded into its domain type.
    #[error("stored {what} is invalid")]
    Corrupt {
        /// What was being decoded, for example `"study status"`.
        what: &'static str,
        /// Why decoding failed.
        #[source]
        source: Cause,
    },
    /// A stored column holds a value that is not a known variant.
    #[error("stored {field} has unrecognized value {value:?}")]
    UnknownValue {
        /// Column or field that held the value.
        field: &'static str,
        /// The unrecognized value.
        value: String,
    },
    /// A numeric value does not fit the store's column type.
    #[error("{field} is out of range")]
    OutOfRange {
        /// Name of the value that is out of range.
        field: &'static str,
        /// The failed conversion.
        #[source]
        source: TryFromIntError,
    },
}

impl RepositoryError {
    /// Classifies `source` as a failure to connect to the store.
    pub fn connection(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Connection(Box::new(source))
    }

    /// Classifies `source` as a failed store operation.
    pub fn storage(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Storage(Box::new(source))
    }

    /// Classifies `source` as a stored value of kind `what` that cannot be decoded.
    pub fn corrupt(
        what: &'static str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Corrupt {
            what,
            source: Box::new(source),
        }
    }
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

/// Per-arm randomization tally for a study within one environment.
///
/// Computed by the store (`GROUP BY` arm) so balance figures reflect every
/// record, not just a paginated page of the newest rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmTally {
    pub arm_id: Uuid,
    pub count: u64,
}

/// Persistence for randomization assignment records.
///
/// All reads and the uniqueness check are partitioned by [`Environment`] so
/// that non-production assignments never mix with production trial data.
#[async_trait]
pub trait RandomizationRepository: Send + Sync {
    /// Store a new randomization record. The record carries its own environment.
    async fn save(&self, randomization: &Randomization) -> Result<Randomization, RepositoryError>;
    /// List randomizations for a study within one environment, with pagination.
    async fn find_by_study(
        &self,
        study_id: &Uuid,
        environment: Environment,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Randomization>, RepositoryError>;
    /// Tally randomizations per arm for a study within one environment.
    ///
    /// Aggregated in the store so callers get whole-study counts without
    /// paging through every record.
    async fn count_by_arm(
        &self,
        study_id: &Uuid,
        environment: Environment,
    ) -> Result<Vec<ArmTally>, RepositoryError>;
    /// Check if a randomization already exists for this study + environment +
    /// unit combination. Returns the existing record if found.
    ///
    /// `unit_key` is the delimiter-joined string form of the randomization
    /// [`Unit`](crate::random::Unit) (see `String::from(&Unit)`), not a hash.
    async fn exists_for_unit(
        &self,
        study_id: &Uuid,
        environment: Environment,
        unit_key: &str,
    ) -> Result<Option<Randomization>, RepositoryError>;
    /// Delete all non-production randomizations for a study, returning the
    /// count removed. Production records are never touched.
    async fn purge_nonproduction(&self, study_id: &Uuid) -> Result<u64, RepositoryError>;
}

/// Cache for permuted blocks (Redis hot path).
///
/// Blocks are partitioned by [`Environment`]: production and non-production
/// draw from independent block sequences, so a test randomization can never
/// advance or consume the production allocation sequence.
#[async_trait]
pub trait BlockCacheRepository: Send + Sync {
    /// Retrieve the current block for an environment/study/stratum combination.
    async fn get_block(
        &self,
        environment: Environment,
        study_id: &Uuid,
        stratum_key: &str,
    ) -> Result<Option<RandomPermutedBlock>, RepositoryError>;
    /// Store a block in the cache for the given environment.
    async fn save_block(
        &self,
        environment: Environment,
        block: &RandomPermutedBlock,
    ) -> Result<(), RepositoryError>;
    /// Atomically consume the next assignment from a cached block.
    /// Returns `None` if the block is exhausted or not found.
    async fn consume_next(
        &self,
        environment: Environment,
        study_id: &Uuid,
        stratum_key: &str,
    ) -> Result<Option<u32>, RepositoryError>;
    /// Delete all non-production cached blocks for a study, returning the count
    /// removed. Production blocks are never touched. Used when resetting test
    /// data so a subsequent non-production run starts from a fresh sequence.
    async fn purge_nonproduction(&self, study_id: &Uuid) -> Result<u64, RepositoryError>;
}

/// Persistence for Epic locator → study mappings.
#[async_trait]
pub trait LocatorRepository: Send + Sync {
    /// Create or update a locator → study mapping.
    async fn save_mapping(&self, locator: u64, study_id: &Uuid) -> Result<(), RepositoryError>;
    /// Look up which study a locator belongs to.
    async fn find_study_id(&self, locator: u64) -> Result<Option<Uuid>, RepositoryError>;
}

/// A user's membership in a study.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyMember {
    /// Canonical user identifier (Google email from `X-Vouch-User`).
    pub user_id: String,
    /// Membership role: `"owner"` or `"member"`.
    pub role: String,
    /// When the user was granted access.
    pub joined_at: DateTime<Utc>,
}

/// Persistence for user ↔ study associations.
#[async_trait]
pub trait UserStudyRepository: Send + Sync {
    /// Associate a user with a study in the given role, upserting the role if
    /// the association already exists.
    async fn add_user(
        &self,
        user_id: &str,
        study_id: &Uuid,
        role: &str,
    ) -> Result<(), RepositoryError>;
    /// Remove a user's association with a study.
    async fn remove_user(&self, user_id: &str, study_id: &Uuid) -> Result<(), RepositoryError>;
    /// Return the user's role in the study, or `None` if they have no access.
    async fn get_role(
        &self,
        user_id: &str,
        study_id: &Uuid,
    ) -> Result<Option<String>, RepositoryError>;
    /// List all members of a study, ordered by when they joined.
    async fn list_members(&self, study_id: &Uuid) -> Result<Vec<StudyMember>, RepositoryError>;
}

/// Append-only persistence for study audit events.
///
/// Events are written once and never mutated. Reads return the study's history
/// newest-first so the history view can render most-recent activity at the top.
#[async_trait]
pub trait AuditRepository: Send + Sync {
    /// Append a new audit event.
    async fn append(&self, event: &AuditEvent) -> Result<(), RepositoryError>;
    /// List a study's audit events, ordered newest-first.
    async fn list_by_study(&self, study_id: &Uuid) -> Result<Vec<AuditEvent>, RepositoryError>;
}

/// Persistence for patient demographics used by balance assessment.
///
/// Demographics are synced in bulk from an external source (Caboodle) and read
/// back joined with randomization arm assignments for parity reporting.
#[async_trait]
pub trait DemographicsRepository: Send + Sync {
    /// Bulk upsert a batch of demographic records, refreshing existing rows.
    async fn upsert_batch(&self, batch: &[PatientDemographics]) -> Result<(), RepositoryError>;
    /// Fetch demographics for a set of patient IDs.
    async fn find_by_patient_ids(
        &self,
        patient_ids: &[String],
    ) -> Result<Vec<PatientDemographics>, RepositoryError>;
    /// Fetch demographics for all randomized patients in a study, joined with
    /// their arm assignment.
    async fn find_by_study(
        &self,
        study_id: &Uuid,
    ) -> Result<Vec<PatientDemographicsWithArm>, RepositoryError>;
}

/// Cache for study configs on the CDS Hooks hot path.
#[async_trait]
pub trait StudyCache: Send + Sync {
    /// Look up cached study_id for a locator.
    async fn get_study_id_for_locator(&self, locator: u64)
    -> Result<Option<Uuid>, RepositoryError>;
    /// Cache a locator → study_id mapping.
    async fn cache_locator_mapping(
        &self,
        locator: u64,
        study_id: &Uuid,
    ) -> Result<(), RepositoryError>;
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

/// Orchestrates the randomization flow for a study + request within an
/// [`Environment`]. The environment is resolved by the caller (from the CDS
/// Hooks `fhirServer` URL) and gates whether the request may proceed.
#[async_trait]
pub trait RandomizationService: Send + Sync {
    async fn randomize(
        &self,
        study: &Study,
        environment: Environment,
        request: &RandomizationRequest,
    ) -> Result<Randomization, RandomizationError>;
}
