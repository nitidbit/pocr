//! Two-pass v4 → v7 identifier remapping and its persisted manifest.
//!
//! Every legacy UUIDv4 is replaced by a v7 minted from the record's **original**
//! timestamp (not the migration instant) via
//! [`uuid::Builder::from_unix_timestamp_millis`], so the new ids sort in true
//! historical order. Minting draws fresh random bytes, so a naive re-run would
//! produce different ids; idempotency therefore depends on the manifest — on a
//! re-run the existing [`Manifest`] is loaded and its maps reused verbatim.
//!
//! The manifest holds all three old→new maps (`study`, `arm`, `randomization`)
//! and is the migration's audit trail and reconciliation aid.

use std::collections::HashMap;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};
use uuid::{Builder, Uuid};

/// Mint a v7 UUID whose timestamp component is `millis` (unix epoch
/// milliseconds), with fresh random bytes for the remaining entropy.
///
/// This is the historical-order minting the migration relies on; contrast with
/// `Uuid::now_v7`, which would stamp every record at the migration instant.
pub fn v7_from_millis(millis: u64, rng: &mut impl Rng) -> Uuid {
    let mut random_bytes = [0u8; 10];
    rng.fill(&mut random_bytes);
    Builder::from_unix_timestamp_millis(millis, &random_bytes).into_uuid()
}

/// The old→new id maps, persisted as `migration-manifest.json`.
///
/// Keys are legacy UUIDv4 strings; values are the minted v7 ids.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    /// Legacy study id → new v7 study id.
    pub studies: HashMap<String, Uuid>,
    /// Legacy arm id → new v7 arm id.
    pub arms: HashMap<String, Uuid>,
    /// Legacy randomization id → new v7 randomization id.
    pub randomizations: HashMap<String, Uuid>,
}

impl Manifest {
    /// Look up (or mint) the v7 for a legacy study id at `created_at_millis`.
    ///
    /// Reusing an existing entry is what makes a manifest-backed re-run
    /// idempotent; a fresh entry is minted only when the id is unseen.
    pub fn study_id(&mut self, legacy: &str, created_at_millis: u64, rng: &mut impl Rng) -> Uuid {
        upsert(&mut self.studies, legacy, created_at_millis, rng)
    }

    /// Look up (or mint) the v7 for a legacy arm id, timestamped by its owning
    /// study's `created_at`.
    pub fn arm_id(&mut self, legacy: &str, study_created_at_millis: u64, rng: &mut impl Rng) -> Uuid {
        upsert(&mut self.arms, legacy, study_created_at_millis, rng)
    }

    /// Look up (or mint) the v7 for a legacy randomization id at its
    /// `issued_at`.
    pub fn randomization_id(
        &mut self,
        legacy: &str,
        issued_at_millis: u64,
        rng: &mut impl Rng,
    ) -> Uuid {
        upsert(&mut self.randomizations, legacy, issued_at_millis, rng)
    }
}

/// Return the existing mapping for `legacy`, or mint a new v7 at `millis`.
fn upsert(map: &mut HashMap<String, Uuid>, legacy: &str, millis: u64, rng: &mut impl Rng) -> Uuid {
    if let Some(existing) = map.get(legacy) {
        return *existing;
    }
    let minted = v7_from_millis(millis, rng);
    map.insert(legacy.to_string(), minted);
    minted
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::SmallRng;
    use rand::SeedableRng;

    fn rng() -> SmallRng {
        SmallRng::seed_from_u64(7)
    }

    #[test]
    fn minted_uuid_is_v7() {
        let id = v7_from_millis(1_772_550_371_000, &mut rng());
        assert_eq!(id.get_version_num(), 7);
    }

    #[test]
    fn minted_uuid_carries_source_timestamp() {
        let millis = 1_772_550_371_000;
        let id = v7_from_millis(millis, &mut rng());
        // v7 timestamp is the top 48 bits, unix epoch milliseconds.
        let bytes = id.as_bytes();
        let ts = ((bytes[0] as u64) << 40)
            | ((bytes[1] as u64) << 32)
            | ((bytes[2] as u64) << 24)
            | ((bytes[3] as u64) << 16)
            | ((bytes[4] as u64) << 8)
            | (bytes[5] as u64);
        assert_eq!(ts, millis);
    }

    #[test]
    fn earlier_timestamp_sorts_before_later() {
        let early = v7_from_millis(1_000_000_000_000, &mut rng());
        let late = v7_from_millis(2_000_000_000_000, &mut rng());
        assert!(early < late, "v7 ids must sort by source timestamp");
    }

    #[test]
    fn manifest_reuses_existing_mapping() {
        let mut manifest = Manifest::default();
        let first = manifest.study_id("legacy-a", 1_000, &mut rng());
        // A later call with a *different* timestamp must still return the
        // original id — this is the idempotency guarantee for re-runs.
        let second = manifest.study_id("legacy-a", 9_999, &mut rng());
        assert_eq!(first, second);
    }

    #[test]
    fn manifest_mints_distinct_ids_per_legacy_id() {
        let mut manifest = Manifest::default();
        let mut rng = rng();
        let a = manifest.arm_id("arm-a", 1_000, &mut rng);
        let b = manifest.arm_id("arm-b", 1_000, &mut rng);
        assert_ne!(a, b, "distinct legacy ids at the same instant must differ");
    }

    #[test]
    fn manifest_roundtrips_through_json() {
        let mut manifest = Manifest::default();
        manifest.study_id("s", 1_000, &mut rng());
        manifest.randomization_id("r", 2_000, &mut rng());

        let json = serde_json::to_string(&manifest).unwrap();
        let restored: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, restored);
    }
}
