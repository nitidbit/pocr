//! Randomization primitives for clinical trial arm assignment.
//!
//! Provide permuted-block randomization with configurable arm ratios,
//! optional stratification, and flexible unit identification. The primary
//! entry point is [`RandomPermutedBlock`] for block generation and
//! [`Randomization`] for recording issued assignments.

mod block;
mod randomization;
mod request;
mod stratum;
mod unit;
mod arm;
mod key;

pub use arm::Arm;
pub use block::RandomPermutedBlock;
pub use key::{FhirPath, KeyVariant};
pub use randomization::Randomization;
pub use request::RandomizationRequest;
pub use stratum::{SimpleStratum, Strata, Stratum};
pub use unit::{FhirPathValue, SimpleUnit, Unit};

#[cfg(test)]
mod tests {}
