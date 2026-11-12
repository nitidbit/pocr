use std::fmt;

/// System columns POCR adds to every dataset row; field names may not reuse them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemColumn {
    /// The randomization's identifier.
    RandomizationId,
    /// The assigned arm's identifier.
    ArmId,
    /// The assigned arm's display name.
    ArmName,
}

impl SystemColumn {
    /// System columns in CSV header order.
    pub const ALL: [Self; 3] = [Self::RandomizationId, Self::ArmId, Self::ArmName];

    /// Returns the column's wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RandomizationId => "randomization_id",
            Self::ArmId => "arm_id",
            Self::ArmName => "arm_name",
        }
    }

    /// Returns the system column named `name`, if any.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|column| column.as_str() == name)
    }
}

impl fmt::Display for SystemColumn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
