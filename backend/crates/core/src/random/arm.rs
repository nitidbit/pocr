use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A treatment arm within a randomization study.
///
/// Each arm has a relative allocation ratio that determines how many
/// subjects are assigned to it within each permuted block.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Arm {
    /// Create a new arm with a generated UUIDv7 and enforced minimum ratio of 1.
    pub fn new(name: impl Into<String>, ratio: u32) -> Self {
        Self {
            id: Uuid::now_v7(),
            name: name.into(),
            ratio: ratio.max(1),
            locator: None,
        }
    }
}