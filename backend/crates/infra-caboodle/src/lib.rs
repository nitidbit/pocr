//! Caboodle (Epic MS-SQL data warehouse) integration layer.
//!
//! Provide connection management and query functions for pulling
//! patient demographics from Caboodle's dimensional model.

mod config;
mod connection;
mod datasets;
mod error;
mod queries;

pub use config::CaboodleConfig;
pub use connection::CaboodleClient;
pub use datasets::execute_dataset;
pub use error::{CaboodleError, CaboodleResult, DriverFailure};
pub use queries::{
    CaboodleDemographics, fetch_bmi, fetch_demographics, fetch_insurance, fetch_races,
};
