//! Caboodle (Epic MS-SQL data warehouse) integration layer.
//!
//! Provide connection management and query functions for pulling
//! patient demographics from Caboodle's dimensional model.

pub mod config;
pub mod connection;
pub mod error;
pub mod queries;

pub use config::CaboodleConfig;
pub use connection::CaboodleClient;
pub use error::CaboodleError;
pub use queries::CaboodleDemographics;
