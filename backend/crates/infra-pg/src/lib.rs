//! PostgreSQL persistence layer for POCR.
//!
//! Implements the repository port traits defined in `core::ports` against
//! PostgreSQL via sqlx.

mod audit_repo;
mod datasets;
mod demographics;
mod error;
mod locator_repo;
mod pool;
mod randomization_repo;
mod study_repo;
mod user_study_repo;

pub use audit_repo::PgAuditRepository;
pub use datasets::{
    DatabaseFailure, PgDatasetRepository, RefreshLock, RefreshRun, RefreshStoreError, RefreshTarget,
};
pub use demographics::PgDemographicsRepository;
pub use locator_repo::PgLocatorRepository;
pub use pool::{create_pool, run_migrations};
pub use randomization_repo::PgRandomizationRepository;
pub use study_repo::PgStudyRepository;
pub use user_study_repo::PgUserStudyRepository;
