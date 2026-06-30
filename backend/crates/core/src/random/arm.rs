use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A treatment arm within a randomization study.
///
/// Each arm has a relative allocation ratio that determines how many
/// subjects are assigned to it within each permuted block.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Arm {
    /// Unique identifier for this arm.
    pub id: Uuid,
    /// Human-readable label for this arm (e.g., "Treatment A").
    #[serde(default)]
    pub name: String,
    /// Relative allocation weight. Must be at least 1. A study with arms
    /// of ratio 2 and 1 assigns subjects in a 2:1 proportion.
    pub ratio: u32,
    /// Epic LGL (Locator) ID mapped to this arm, if applicable.
    pub locator: Option<u64>,
}