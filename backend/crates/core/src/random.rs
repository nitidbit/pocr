//! Randomization primitives for clinical trial arm assignment.
//!
//! Provides permuted-block randomization with configurable arm ratios,
//! optional stratification, and flexible unit identification. The primary
//! entry point is [`RandomPermutedBlock`] for block generation and
//! [`Randomization`] for recording issued assignments.

mod arm;
mod block;
mod key;
mod randomization;
mod request;
mod stratum;
mod unit;

pub use arm::Arm;
pub use block::RandomPermutedBlock;
pub use key::{FhirPath, KeyVariant};
pub use randomization::Randomization;
pub use request::{EpicIds, RandomizationRequest};
pub use stratum::{SimpleStratum, Strata, Stratum};
pub use unit::{FhirPathValue, SimpleUnit, Unit};
