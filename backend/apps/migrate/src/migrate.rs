//! Migration orchestrator: the two-pass ETL tying source, transforms, ids, and
//! sink together.
//!
//! Pass 1 builds (or reuses, from an existing manifest) the old→new v7 id maps
//! for studies and arms. Pass 2 applies those maps to every record and writes
//! the POCR rows and Redis blocks. Randomization ids are minted lazily in
//! Pass 2 from each record's `issued_at`. The manifest is persisted at the end
//! so a re-run reuses identical ids and the whole migration is idempotent.

use std::path::{Path, PathBuf};

use rand::rngs::StdRng;
use tracing::{info, instrument, warn};

use pocr_core::study::StudyStatus;

use crate::ids::Manifest;
use crate::legacy::LegacyStudy;
use crate::sink::{PocrSink, SinkError};
use crate::source::{LegacySource, SourceError};
use crate::transform::{
    self, TransformError, transform_block, transform_randomization, transform_study,
};

/// Errors from the migration run.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// Reading the legacy Redis failed.
    #[error(transparent)]
    Source(#[from] SourceError),
    /// Writing to a POCR store failed.
    #[error(transparent)]
    Sink(#[from] SinkError),
    /// Translating a record failed.
    #[error(transparent)]
    Transform(#[from] TransformError),
    /// Reading or writing the manifest file failed.
    #[error("manifest I/O error at {path}: {source}")]
    Manifest {
        /// The manifest path.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The manifest file held invalid JSON.
    #[error("manifest parse error: {0}")]
    ManifestParse(#[from] serde_json::Error),
    /// A block referenced a study id absent from the study map, so its key
    /// could not be rewritten.
    #[error("block references unknown legacy study id: {0}")]
    UnknownBlockStudy(String),
    /// A randomization referenced an arm id absent from its study's config —
    /// migrating it would silently drop a live assignment, so the run aborts.
    #[error("randomization {id} references arm {arm_id} not in study {study_id}'s config")]
    UnmappedRandomizationArm {
        /// Legacy randomization id.
        id: String,
        /// Legacy arm id that was not found in the arm map.
        arm_id: String,
        /// Legacy study id the randomization belongs to.
        study_id: String,
    },
    /// No migrated study resolved to `active`, so the live study is missing —
    /// a strong signal the wrong Redis was pointed at.
    #[error("no migrated study resolved to 'active'; refusing to proceed")]
    NoActiveStudy,
}

/// Counts of records migrated, for count-parity validation.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MigrationCounts {
    /// Studies written.
    pub studies: usize,
    /// Randomizations written.
    pub randomizations: usize,
    /// Locator mappings written.
    pub locators: usize,
    /// Blocks written.
    pub blocks: usize,
}

/// Load an existing manifest from `path`, or return a fresh empty one.
///
/// A present manifest makes the run idempotent (ids are reused); its absence
/// means a clean from-scratch migration.
pub fn load_manifest(path: &Path) -> Result<Manifest, MigrateError> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(serde_json::from_str(&contents)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            info!(path = %path.display(), "no existing manifest; starting a fresh migration");
            Ok(Manifest::default())
        }
        Err(source) => Err(MigrateError::Manifest {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Persist the manifest to `path` as pretty JSON.
pub fn save_manifest(path: &Path, manifest: &Manifest) -> Result<(), MigrateError> {
    let json = serde_json::to_string_pretty(manifest)?;
    std::fs::write(path, json).map_err(|source| MigrateError::Manifest {
        path: path.to_path_buf(),
        source,
    })
}

/// Milliseconds since the unix epoch for an epoch-**seconds** value, clamped at
/// zero. Used to seed v7 minting from a study's `created_at`.
fn seconds_to_millis(secs: i64) -> u64 {
    (secs.max(0) as u64).saturating_mul(1000)
}

/// Pass 1: populate the study and arm id maps in `manifest` from `studies`.
///
/// Each study mints (or reuses) a v7 from its `created_at`; each of its arms
/// mints (or reuses) a v7 timestamped by the same `created_at`. Also asserts at
/// least one study resolves to [`StudyStatus::Active`] — the live study.
#[instrument(skip_all, fields(studies = studies.len()))]
fn build_id_maps(
    manifest: &mut Manifest,
    studies: &[LegacyStudy],
    rng: &mut impl rand::Rng,
) -> Result<(), MigrateError> {
    let mut saw_active = false;
    for study in studies {
        if transform::transform_status(study.status)? == StudyStatus::Active {
            saw_active = true;
        }
        let created_millis = seconds_to_millis(study.created_at);
        manifest.study_id(&study.id, created_millis, rng);
        for arm in &study.randomization_config.arms {
            manifest.arm_id(&arm.id, created_millis, rng);
        }
    }
    if !saw_active {
        return Err(MigrateError::NoActiveStudy);
    }
    Ok(())
}

/// Run the full two-pass migration.
///
/// Reads every legacy record via `source`, builds/reuses id maps in `manifest`,
/// writes transformed rows and blocks via `sink`, and returns the migrated
/// counts for validation. The caller persists `manifest` afterward.
#[instrument(skip_all)]
pub async fn migrate(
    source: &LegacySource,
    sink: &PocrSink,
    manifest: &mut Manifest,
) -> Result<MigrationCounts, MigrateError> {
    // A Send RNG is required because it is held across await points. Ids only
    // need uniqueness plus the correct timestamp prefix, so any CSPRNG suffices.
    let mut rng: StdRng = rand::make_rng();

    // ── Pass 1: id maps from studies ────────────────────────────────────────
    let studies = source.studies().await?;
    build_id_maps(manifest, &studies, &mut rng)?;
    info!(
        studies = manifest.studies.len(),
        arms = manifest.arms.len(),
        "pass 1 complete: id maps built"
    );

    let mut counts = MigrationCounts::default();

    // ── Pass 2a: studies (+ owner rows) ─────────────────────────────────────
    for legacy in &studies {
        let new_id = *manifest
            .studies
            .get(&legacy.id)
            .expect("study id minted in pass 1");
        let study = transform_study(legacy, new_id, &manifest.arms)?;
        sink.write_study(&study).await?;
        counts.studies += 1;

        if let Some(user) = &legacy.created_by
            && !user.id.is_empty()
        {
            sink.write_owner(&user.id, &new_id).await?;
        }
    }

    // ── Pass 2b: randomizations ─────────────────────────────────────────────
    for legacy in source.randomizations().await? {
        let Some(&new_study_id) = manifest.studies.get(&legacy.study_id) else {
            warn!(
                study_id = legacy.study_id,
                "skipping randomization for a study with no migrated config"
            );
            continue;
        };
        // An unmapped arm on a record whose study *was* migrated means the
        // study config and its assignments disagree — skipping would silently
        // drop a live randomization, so abort rather than lose data.
        let Some(&new_arm_id) = manifest.arms.get(&legacy.arm.id) else {
            return Err(MigrateError::UnmappedRandomizationArm {
                id: legacy.id.clone(),
                arm_id: legacy.arm.id.clone(),
                study_id: legacy.study_id.clone(),
            });
        };
        let issued_millis = legacy.issued_at.timestamp_millis().max(0) as u64;
        let new_id = manifest.randomization_id(&legacy.id, issued_millis, &mut rng);
        let rec = transform_randomization(&legacy, new_id, new_study_id, new_arm_id);
        sink.write_randomization(&rec).await?;
        counts.randomizations += 1;
    }

    // ── Pass 2c: locator mappings ───────────────────────────────────────────
    for record in source.locators().await? {
        let Some(&new_study_id) = manifest.studies.get(&record.mapping.study_id) else {
            warn!(
                study_id = record.mapping.study_id,
                locator = record.locator,
                "skipping locator for a study with no migrated config"
            );
            continue;
        };
        sink.write_locator(record.locator, &new_study_id).await?;
        counts.locators += 1;
    }

    // ── Pass 2d: blocks ─────────────────────────────────────────────────────
    for legacy in source.blocks().await? {
        let new_study_id = *manifest
            .studies
            .get(&legacy.study_id)
            .ok_or_else(|| MigrateError::UnknownBlockStudy(legacy.study_id.clone()))?;
        let block = transform_block(&legacy, new_study_id)?;
        sink.write_block(&block).await?;
        counts.blocks += 1;
    }

    info!(?counts, "pass 2 complete: all records written");
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    use uuid::Uuid;

    fn rng() -> SmallRng {
        SmallRng::seed_from_u64(1)
    }

    fn study_json(id: &str, arm_id: &str, status: i32, created_at: i64) -> LegacyStudy {
        serde_json::from_str(&format!(
            r#"{{"id":"{id}","name":"n","description":"d","randomization_config":{{"strategy":2,"block_size":0,"arms":[{{"id":"{arm_id}","ratio":1,"locator":null}}],"keys":[],"strata":[]}},"status":{status},"created_at":{created_at},"updated_at":{created_at}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn build_id_maps_mints_studies_and_arms() {
        let mut manifest = Manifest::default();
        let studies = vec![study_json("s1", "a1", 2, 1_772_550_371)];
        build_id_maps(&mut manifest, &studies, &mut rng()).unwrap();

        assert!(manifest.studies.contains_key("s1"));
        assert!(manifest.arms.contains_key("a1"));
    }

    #[test]
    fn build_id_maps_requires_an_active_study() {
        let mut manifest = Manifest::default();
        // All draft (status 1): the live study is absent.
        let studies = vec![study_json("s1", "a1", 1, 1)];
        let err = build_id_maps(&mut manifest, &studies, &mut rng()).unwrap_err();
        assert!(matches!(err, MigrateError::NoActiveStudy));
    }

    #[test]
    fn build_id_maps_is_idempotent_across_calls() {
        let mut manifest = Manifest::default();
        let studies = vec![study_json("s1", "a1", 2, 1_772_550_371)];
        build_id_maps(&mut manifest, &studies, &mut rng()).unwrap();
        let study_id = manifest.studies["s1"];
        let arm_id = manifest.arms["a1"];

        // Re-running against the same manifest must not re-mint.
        build_id_maps(&mut manifest, &studies, &mut rng()).unwrap();
        assert_eq!(manifest.studies["s1"], study_id);
        assert_eq!(manifest.arms["a1"], arm_id);
    }

    #[test]
    fn seconds_to_millis_scales_and_clamps() {
        assert_eq!(seconds_to_millis(1), 1000);
        assert_eq!(seconds_to_millis(-5), 0);
    }

    #[test]
    fn manifest_roundtrips_to_disk() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("pocr-migrate-manifest-{}.json", Uuid::now_v7()));

        let mut manifest = Manifest::default();
        manifest.study_id("s1", 1_000, &mut rng());
        save_manifest(&path, &manifest).unwrap();

        let loaded = load_manifest(&path).unwrap();
        assert_eq!(loaded, manifest);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_manifest_returns_default_when_absent() {
        let path = std::env::temp_dir().join(format!("pocr-missing-{}.json", Uuid::now_v7()));
        let manifest = load_manifest(&path).unwrap();
        assert_eq!(manifest, Manifest::default());
    }
}
