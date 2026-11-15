//! Caboodle (Epic MS-SQL data warehouse) integration: connection management and dataset procedure execution.

mod config;
mod connection;
mod datasets;
mod error;

pub use config::CaboodleConfig;
pub use connection::CaboodleClient;
pub use datasets::execute_dataset;
pub use error::{CaboodleError, CaboodleResult, DriverFailure};
