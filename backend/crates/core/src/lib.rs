//! Domain model for the POCR clinical trial randomization core.
//!
//! This crate defines the core types and logic for permuted-block
//! randomization, including arm definitions, stratification, unit
//! identification, and error handling. It is infrastructure-agnostic
//! and contains no I/O.

pub mod error;
pub mod random;
