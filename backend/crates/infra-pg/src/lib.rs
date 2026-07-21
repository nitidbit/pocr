//! PostgreSQL persistence layer for POCR.
//!
//! Implements the repository port traits defined in `core::ports` against
//! PostgreSQL via sqlx.

pub mod error;
pub mod locator_repo;
pub mod pool;
pub mod randomization_repo;
pub mod study_repo;
pub mod user_study_repo;
pub mod demographics;

pub use locator_repo::PgLocatorRepository;
pub use pool::{create_pool, run_migrations};
pub use randomization_repo::PgRandomizationRepository;
pub use study_repo::PgStudyRepository;
pub use user_study_repo::PgUserStudyRepository;
pub use demographics::PgDemographicsRepository;
