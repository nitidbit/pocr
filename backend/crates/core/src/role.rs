//! Study access roles.

use serde::{Deserialize, Serialize};

/// A study access role, ordered so that `Owner >= Member`.
///
/// Declaration order defines the ordering: [`Role::Member`] is the lower
/// privilege, [`Role::Owner`] the higher. The serialized form is the lowercase
/// stored form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Read-only access to the study.
    Member,
    /// Full access, including membership management.
    Owner,
}

impl Role {
    /// The wire and database string form of the role.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Owner => "owner",
        }
    }

    /// Parses the lowercase stored form, returning `None` for any other string.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "member" => Some(Self::Member),
            "owner" => Some(Self::Owner),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_known_roles() {
        assert_eq!(Role::parse("member"), Some(Role::Member));
        assert_eq!(Role::parse("owner"), Some(Role::Owner));
    }

    #[test]
    fn parse_rejects_unknown_role() {
        assert_eq!(Role::parse("admin"), None);
        assert_eq!(Role::parse(""), None);
        assert_eq!(Role::parse("Owner"), None);
    }

    #[test]
    fn owner_outranks_member() {
        assert!(Role::Owner > Role::Member);
    }

    #[test]
    fn serializes_as_lowercase() {
        assert_eq!(serde_json::to_value(Role::Owner).unwrap(), "owner");
        assert_eq!(
            serde_json::from_value::<Role>("member".into()).unwrap(),
            Role::Member
        );
        serde_json::from_value::<Role>("Owner".into()).unwrap_err();
    }
}
