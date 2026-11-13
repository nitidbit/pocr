//! gRPC API for the POCR engine.

/// Returns the sum of `left` and `right`.
#[must_use]
pub fn add(left: u64, right: u64) -> u64 {
    left + right
}
