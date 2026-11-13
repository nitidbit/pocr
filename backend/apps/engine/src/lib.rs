//! Randomization engine: the [`Engine`] service, its health checks, and the
//! startup error types shared with the `pocr-engine` binary.

mod error;
mod health;
mod service;

pub use error::{EngineError, EngineResult};
pub use health::{PgHealthCheck, RedisHealthCheck};
pub use service::Engine;
