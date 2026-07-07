//! Redis caching layer for POCR.
//!
//! Provides a block cache for permuted-block randomization hot paths
//! and a study config cache for CDS Hooks locator lookups.

pub mod block_cache;
pub mod connection;
pub mod study_cache;

pub use block_cache::RedisBlockCache;
pub use connection::RedisPool;
pub use study_cache::RedisStudyCache;
