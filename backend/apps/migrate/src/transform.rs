//! Pure transforms from legacy proto-JSON source models to `pocr-core` models.
//!
//! Every function here is deterministic given its inputs and does no I/O. The
//! caller supplies the already-remapped v7 identifiers (minted in
//! [`crate::ids`]); these functions apply them and translate the proto integer
//! enums, `oneof` key shapes, and dynamic-block sentinel into the hand-written
//! core representation.
//!
//! Hashing parity is not re-implemented here: `unit`/`strata`/`Stratum` shapes
//! are shared with core, so `unit_hash`/`strata_key` are computed downstream by
//! the same `String::from(&unit)` / `String::from(&strata)` the live engine uses.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use pocr_core::environment::Environment;
use pocr_core::random::{Arm, Key, KeyVariant, RandomPermutedBlock, Randomization};
use pocr_core::study::{RandomizationConfig, RandomizationStrategy, Study, StudyStatus};

use crate::legacy::{
    LegacyArm, LegacyBlock, LegacyKey, LegacyKeyVariant, LegacyRandomization,
    LegacyRandomizationConfig, LegacyStudy,
};

/// Errors from translating a legacy record into a core model.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransformError {
    /// A proto `StudyStatus` ordinal outside the known `1..=3` range.
    #[error("unknown study status ordinal: {0}")]
    UnknownStatus(i32),
    /// A proto `RandomizationStrategy` ordinal outside the known `1..=2` range.
    #[error("unknown randomization strategy ordinal: {0}")]
    UnknownStrategy(i32),
    /// A unix-epoch value that does not correspond to a valid timestamp.
    #[error("invalid unix timestamp (seconds): {0}")]
    InvalidTimestamp(i64),
    /// A randomization referenced an arm id absent from the study's arm map.
    #[error("randomization references unknown arm id: {0}")]
    UnknownArm(String),
}

/// Map a proto `StudyStatus` ordinal to a [`StudyStatus`].
///
/// `1 → Draft`, `2 → Active`, `3 → Closed`. The legacy terminal `Inactive`
/// (`3`) becomes the POCR terminal `Closed`; `paused` has no legacy source.
/// `0` (`Unspecified`) and any other ordinal are rejected.
pub fn transform_status(ordinal: i32) -> Result<StudyStatus, TransformError> {
    match ordinal {
        1 => Ok(StudyStatus::Draft),
        2 => Ok(StudyStatus::Active),
        3 => Ok(StudyStatus::Closed),
        other => Err(TransformError::UnknownStatus(other)),
    }
}

/// Map a proto `RandomizationStrategy` ordinal to a [`RandomizationStrategy`].
///
/// `1 → Simple`, `2 → Block`. `0` (`Unspecified`) and any other ordinal are
/// rejected.
pub fn transform_strategy(ordinal: i32) -> Result<RandomizationStrategy, TransformError> {
    match ordinal {
        1 => Ok(RandomizationStrategy::Simple),
        2 => Ok(RandomizationStrategy::Block),
        other => Err(TransformError::UnknownStrategy(other)),
    }
}

/// Translate the legacy dynamic-block sentinel: `0 → None`, `n → Some(n)`.
pub fn transform_block_size(block_size: u32) -> Option<u32> {
    (block_size != 0).then_some(block_size)
}

/// Translate a legacy arm to a core [`Arm`], applying the remapped `new_id`.
///
/// `ratio` and `locator` are preserved; `name` is empty (the legacy model has
/// no arm name).
pub fn transform_arm(legacy: &LegacyArm, new_id: Uuid) -> Arm {
    Arm {
        id: new_id,
        name: String::new(),
        ratio: legacy.ratio,
        locator: legacy.locator,
    }
}

/// Translate a legacy key `oneof` into a core [`Key`] (`Option<KeyVariant>`).
///
/// A `None` variant (unset `oneof`) maps to `None`.
pub fn transform_key(legacy: &LegacyKey) -> Key {
    legacy.variant.as_ref().map(|variant| match variant {
        LegacyKeyVariant::PatientId {} => KeyVariant::PatientId,
        LegacyKeyVariant::EncounterId {} => KeyVariant::EncounterId,
        LegacyKeyVariant::ProviderId {} => KeyVariant::ProviderId,
        LegacyKeyVariant::FhirPath(fp) => KeyVariant::FhirPath(pocr_core::random::FhirPath {
            id: fp.id.clone(),
            name: fp.name.clone(),
            context: fp.context.clone(),
            path: fp.path.clone(),
        }),
    })
}

/// Convert a unix-epoch **seconds** value to a UTC timestamp.
pub fn epoch_seconds_to_utc(secs: i64) -> Result<DateTime<Utc>, TransformError> {
    DateTime::from_timestamp(secs, 0).ok_or(TransformError::InvalidTimestamp(secs))
}

/// Translate a legacy randomization config, applying the arm-id remap.
///
/// `arm_ids` maps each legacy arm id (UUIDv4 string) to its minted v7 id.
fn transform_config(
    legacy: &LegacyRandomizationConfig,
    arm_ids: &HashMap<String, Uuid>,
) -> Result<RandomizationConfig, TransformError> {
    let strategy = transform_strategy(legacy.strategy)?;
    let arms = legacy
        .arms
        .iter()
        .map(|arm| {
            let new_id = arm_ids
                .get(&arm.id)
                .copied()
                .ok_or_else(|| TransformError::UnknownArm(arm.id.clone()))?;
            Ok(transform_arm(arm, new_id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let keys = legacy.keys.iter().map(transform_key).collect();

    Ok(RandomizationConfig {
        strategy,
        arms,
        keys,
        strata: legacy.strata.clone(),
        block_size: transform_block_size(legacy.block_size),
    })
}

/// Translate a legacy `study:*` record into a core [`Study`].
///
/// `new_id` is the study's minted v7 id; `arm_ids` remaps every arm in the
/// config. Status, strategy, timestamps, and the block-size sentinel are all
/// translated; the config's arms carry their remapped ids.
pub fn transform_study(
    legacy: &LegacyStudy,
    new_id: Uuid,
    arm_ids: &HashMap<String, Uuid>,
) -> Result<Study, TransformError> {
    Ok(Study {
        id: new_id,
        name: legacy.name.clone(),
        description: legacy.description.clone(),
        status: transform_status(legacy.status)?,
        randomization_config: transform_config(&legacy.randomization_config, arm_ids)?,
        created_at: epoch_seconds_to_utc(legacy.created_at)?,
        updated_at: epoch_seconds_to_utc(legacy.updated_at)?,
    })
}

/// Translate a legacy `randomization:*` record into a core [`Randomization`].
///
/// `new_id` is the record's minted v7 id, `new_study_id` the study's, and
/// `new_arm_id` the remapped id of the assigned arm. Every migrated record is
/// tagged [`Environment::Production`]. Epic identifiers are `None` (the legacy
/// engine never captured them).
pub fn transform_randomization(
    legacy: &LegacyRandomization,
    new_id: Uuid,
    new_study_id: Uuid,
    new_arm_id: Uuid,
) -> Randomization {
    Randomization {
        id: new_id,
        study_id: new_study_id,
        environment: Environment::Production,
        strata: legacy.strata.clone(),
        unit: legacy.unit.clone(),
        arm: transform_arm(&legacy.arm, new_arm_id),
        issued_at: legacy.issued_at,
        epic_patient_id: None,
        epic_csn: None,
        epic_user_id: None,
    }
}

/// Translate a legacy `block:*` record into a core [`RandomPermutedBlock`].
///
/// The core type's `index`/`values` are private, so the block is rebuilt by
/// injecting a freshly-minted v7 `id` and the remapped `study_id` into the
/// serialized form and deserializing. `index` and `values` are carried over
/// byte-for-byte so a partially-consumed block resumes mid-sequence; `values`
/// are arm indices and are never remapped.
pub fn transform_block(
    legacy: &LegacyBlock,
    new_study_id: Uuid,
) -> Result<RandomPermutedBlock, serde_json::Error> {
    let value = serde_json::json!({
        "id": Uuid::now_v7(),
        "study_id": new_study_id,
        "stratification": legacy.stratification,
        "index": legacy.index,
        "values": legacy.values,
    });
    serde_json::from_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocr_core::random::{SimpleUnit, Unit};

    #[test]
    fn status_maps_live_study_to_active() {
        assert_eq!(transform_status(2).unwrap(), StudyStatus::Active);
    }

    #[test]
    fn status_maps_draft_and_closed() {
        assert_eq!(transform_status(1).unwrap(), StudyStatus::Draft);
        // Legacy Inactive (3) becomes the terminal Closed.
        assert_eq!(transform_status(3).unwrap(), StudyStatus::Closed);
    }

    #[test]
    fn status_rejects_unspecified_and_unknown() {
        assert_eq!(transform_status(0), Err(TransformError::UnknownStatus(0)));
        assert_eq!(transform_status(9), Err(TransformError::UnknownStatus(9)));
    }

    #[test]
    fn strategy_maps_simple_and_block() {
        assert_eq!(
            transform_strategy(1).unwrap(),
            RandomizationStrategy::Simple
        );
        assert_eq!(transform_strategy(2).unwrap(), RandomizationStrategy::Block);
    }

    #[test]
    fn strategy_rejects_unspecified() {
        assert_eq!(
            transform_strategy(0),
            Err(TransformError::UnknownStrategy(0))
        );
    }

    #[test]
    fn block_size_zero_is_dynamic_none() {
        assert_eq!(transform_block_size(0), None);
        assert_eq!(transform_block_size(6), Some(6));
    }

    #[test]
    fn arm_transform_drops_name_and_keeps_ratio_locator() {
        let legacy = LegacyArm {
            id: "old".to_string(),
            ratio: 3,
            locator: Some(5802),
        };
        let new_id = Uuid::now_v7();
        let arm = transform_arm(&legacy, new_id);

        assert_eq!(arm.id, new_id);
        assert_eq!(arm.name, "");
        assert_eq!(arm.ratio, 3);
        assert_eq!(arm.locator, Some(5802));
    }

    #[test]
    fn key_variant_maps_patient_id() {
        let legacy = LegacyKey {
            variant: Some(LegacyKeyVariant::PatientId {}),
        };
        assert_eq!(transform_key(&legacy), Some(KeyVariant::PatientId));
    }

    #[test]
    fn key_none_variant_maps_to_none() {
        let legacy = LegacyKey { variant: None };
        assert_eq!(transform_key(&legacy), None);
    }

    #[test]
    fn key_variant_maps_fhir_path_fields() {
        let legacy = LegacyKey {
            variant: Some(LegacyKeyVariant::FhirPath(crate::legacy::LegacyFhirPath {
                id: "age".to_string(),
                name: "Age".to_string(),
                context: "Patient".to_string(),
                path: "/age".to_string(),
            })),
        };

        match transform_key(&legacy) {
            Some(KeyVariant::FhirPath(fp)) => {
                assert_eq!(fp.id, "age");
                assert_eq!(fp.name, "Age");
                assert_eq!(fp.context, "Patient");
                assert_eq!(fp.path, "/age");
            }
            other => panic!("expected FhirPath, got {other:?}"),
        }
    }

    #[test]
    fn epoch_conversion_roundtrips() {
        let dt = epoch_seconds_to_utc(1772550371).unwrap();
        assert_eq!(dt.timestamp(), 1772550371);
    }

    #[test]
    fn study_transform_applies_ids_and_enums() {
        let legacy: LegacyStudy = serde_json::from_str(
            r#"{"id":"bd7be9c5-708b-4b9b-b3cf-1d82c647ee16","name":"Test study","description":"Test desc","randomization_config":{"strategy":2,"block_size":0,"arms":[{"id":"d42d45ab-33ae-4d30-9910-678b32478f55","ratio":1,"locator":null},{"id":"7ad8afde-9190-4072-8406-7ff4bea61e50","ratio":1,"locator":null}],"keys":[],"strata":[]},"status":2,"created_at":1772550371,"updated_at":1772550371}"#,
        )
        .unwrap();

        let new_study_id = Uuid::now_v7();
        let arm0 = Uuid::now_v7();
        let arm1 = Uuid::now_v7();
        let arm_ids = HashMap::from([
            ("d42d45ab-33ae-4d30-9910-678b32478f55".to_string(), arm0),
            ("7ad8afde-9190-4072-8406-7ff4bea61e50".to_string(), arm1),
        ]);

        let study = transform_study(&legacy, new_study_id, &arm_ids).unwrap();

        assert_eq!(study.id, new_study_id);
        assert_eq!(study.status, StudyStatus::Active);
        assert_eq!(
            study.randomization_config.strategy,
            RandomizationStrategy::Block
        );
        assert_eq!(study.randomization_config.block_size, None);
        assert_eq!(study.randomization_config.arms[0].id, arm0);
        assert_eq!(study.randomization_config.arms[1].id, arm1);
    }

    #[test]
    fn study_transform_errors_on_unmapped_arm() {
        let legacy: LegacyStudy = serde_json::from_str(
            r#"{"id":"s","name":"n","description":"d","randomization_config":{"strategy":2,"block_size":0,"arms":[{"id":"missing","ratio":1,"locator":null}],"keys":[],"strata":[]},"status":2,"created_at":1,"updated_at":1}"#,
        )
        .unwrap();

        let err = transform_study(&legacy, Uuid::now_v7(), &HashMap::new()).unwrap_err();
        assert_eq!(err, TransformError::UnknownArm("missing".to_string()));
    }

    #[test]
    fn randomization_transform_tags_production_and_remaps_ids() {
        let legacy: LegacyRandomization = serde_json::from_str(
            r#"{"id":"2f1df2e5-71a4-46b2-90a0-4bd22ac260bd","study_id":"228835de-96a7-442f-8fdb-6aeae6af8012","unit":{"kind":"simple","data":{"PatientId":"eaYQYiGc3dMeGffKe8qGUow3"}},"arm":{"id":"3dcce794-5284-4d7c-b773-2e7d18ffb589","ratio":1,"locator":5802},"issued_at":"2026-03-03T15:56:53.232231178Z"}"#,
        )
        .unwrap();

        let new_id = Uuid::now_v7();
        let new_study_id = Uuid::now_v7();
        let new_arm_id = Uuid::now_v7();

        let rec = transform_randomization(&legacy, new_id, new_study_id, new_arm_id);

        assert_eq!(rec.id, new_id);
        assert_eq!(rec.study_id, new_study_id);
        assert_eq!(rec.environment, Environment::Production);
        assert_eq!(rec.arm.id, new_arm_id);
        assert_eq!(rec.arm.locator, Some(5802));
        assert!(rec.epic_patient_id.is_none());
        assert!(matches!(
            rec.unit,
            Unit::Simple(SimpleUnit::PatientId(ref id)) if id == "eaYQYiGc3dMeGffKe8qGUow3"
        ));
        // issued_at is passed through unchanged.
        assert_eq!(rec.issued_at, legacy.issued_at);
    }

    #[test]
    fn block_transform_preserves_index_and_values_byte_for_byte() {
        let legacy: LegacyBlock = serde_json::from_str(
            r#"{"study_id":"228835de-96a7-442f-8fdb-6aeae6af8012","stratification":null,"index":2,"values":[0,1,1,0]}"#,
        )
        .unwrap();

        let new_study_id = Uuid::now_v7();
        let block = transform_block(&legacy, new_study_id).unwrap();

        // Re-serialize to inspect the private index/values without accessors.
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["study_id"], serde_json::json!(new_study_id));
        assert_eq!(json["index"], serde_json::json!(2));
        assert_eq!(json["values"], serde_json::json!([0, 1, 1, 0]));
        assert_eq!(json["stratification"], serde_json::Value::Null);
        // A fresh v7 id was injected.
        assert!(block.id.get_version_num() == 7);
    }
}
