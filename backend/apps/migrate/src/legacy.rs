//! Source models for the legacy POC engine's Redis records.
//!
//! The legacy engine (`cloud-randomization/satellite/service`) serializes
//! prost/tonic-generated proto types to JSON strings: integer enum ordinals,
//! externally-tagged `oneof` shapes, snake_case fields, and unix-epoch `i64`
//! timestamps. These structs deserialize that wire format verbatim so the
//! transform layer can map it onto the hand-written `pocr-core` models.
//!
//! Field shapes are pinned to the ground-truth JSON persisted by the live
//! study (see the deserialization tests below); unknown fields such as the
//! study `organization` are ignored by serde.

use chrono::{DateTime, Utc};
use serde::Deserialize;

use pocr_core::random::Stratum;

/// A legacy `study:{id}` record.
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyStudy {
    /// UUIDv4 string.
    pub id: String,
    /// Human-readable study name.
    pub name: String,
    /// Study description.
    pub description: String,
    /// Randomization configuration.
    pub randomization_config: LegacyRandomizationConfig,
    /// Proto `StudyStatus` ordinal: `Unspecified=0, Draft=1, Active=2, Inactive=3`.
    pub status: i32,
    /// Owning user, if the record carried one. Used to seed a `user_studies`
    /// owner row when a real user id is present.
    #[serde(default)]
    pub created_by: Option<LegacyUser>,
    /// Creation time, unix epoch **seconds**.
    pub created_at: i64,
    /// Last-modified time, unix epoch **seconds**.
    pub updated_at: i64,
}

/// The legacy `created_by` user object.
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyUser {
    /// User identifier (e.g. `"andrew"`).
    pub id: String,
}

/// A legacy `randomization_config` block.
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyRandomizationConfig {
    /// Proto `RandomizationStrategy` ordinal: `Unspecified=0, Simple=1, Block=2`.
    pub strategy: i32,
    /// Block size; `0` means dynamic (per-block) sizing.
    pub block_size: u32,
    /// Treatment arms.
    pub arms: Vec<LegacyArm>,
    /// Unit keys (proto `oneof` shapes).
    pub keys: Vec<LegacyKey>,
    /// Stratification definitions. The legacy `Stratum` shape is byte-identical
    /// to [`pocr_core::random::Stratum`], so it deserializes directly.
    pub strata: Vec<Stratum>,
}

/// A legacy treatment arm. Lacks the `name` field present in the POCR model.
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyArm {
    /// UUIDv4 string.
    pub id: String,
    /// Relative allocation weight.
    pub ratio: u32,
    /// Epic locator (LGL) id mapped to this arm, if any.
    pub locator: Option<u64>,
}

/// A legacy unit key — a proto `oneof` wrapped in a `variant` field.
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyKey {
    /// The active `oneof` arm, if set. Serializes as `{"variant": {...}}`.
    pub variant: Option<LegacyKeyVariant>,
}

/// The legacy key `oneof`, externally tagged by proto variant name.
///
/// Example wire shapes: `{"PatientId":{}}`, `{"FhirPath":{...}}`.
#[derive(Debug, Clone, Deserialize)]
pub enum LegacyKeyVariant {
    /// Identify the subject by patient id.
    PatientId {},
    /// Identify the subject by encounter id.
    EncounterId {},
    /// Identify the subject by provider (user) id.
    ProviderId {},
    /// Extract the subject from a FHIR resource path.
    FhirPath(LegacyFhirPath),
}

/// A legacy FHIR-path key definition.
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyFhirPath {
    /// Path definition id.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// FHIR resource type (e.g. `"Patient"`).
    pub context: String,
    /// JSON pointer within the resource (e.g. `"/gender"`).
    pub path: String,
}

/// A legacy `randomization:{study_id}:{unit}` record.
///
/// The `unit` and `strata` shapes match `pocr-core` exactly, so they
/// deserialize into the core types directly — which is also what guarantees
/// `unit_hash`/`strata_key` parity through the shared `String::from` functions.
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyRandomization {
    /// UUIDv4 string.
    pub id: String,
    /// Owning study, UUIDv4 string.
    pub study_id: String,
    /// Stratification applied at assignment, if any.
    #[serde(default)]
    pub strata: Option<pocr_core::random::Strata>,
    /// The randomization unit (subject identifier).
    pub unit: pocr_core::random::Unit,
    /// The arm the subject was assigned to.
    pub arm: LegacyArm,
    /// Assignment time (RFC3339 string).
    pub issued_at: DateTime<Utc>,
}

/// A legacy `block:{study_id}:{stratification}` permuted-block record.
///
/// Note the absence of an `id` field: the POCR [`RandomPermutedBlock`] requires
/// one, so the transform injects it.
///
/// [`RandomPermutedBlock`]: pocr_core::random::RandomPermutedBlock
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyBlock {
    /// Owning study, UUIDv4 string.
    pub study_id: String,
    /// Serialized stratum key, or `null` for the unstratified block.
    pub stratification: Option<String>,
    /// Cursor position of the next unissued assignment.
    pub index: usize,
    /// Shuffled arm-index sequence. Entries are arm **indices**, never ids.
    pub values: Vec<u32>,
}

/// A legacy `locator:{locator}` record: `{"study_id": "..."}`.
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyLocatorMapping {
    /// The study a locator resolves to, UUIDv4 string.
    pub study_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocr_core::random::{SimpleUnit, Unit};

    // Ground-truth JSON strings below are copied verbatim from the live
    // study's Redis AOF (satellite-data/appendonlydir).

    #[test]
    fn deserializes_study_with_block_strategy() {
        let json = r#"{"id":"bd7be9c5-708b-4b9b-b3cf-1d82c647ee16","name":"Test study","description":"Test desc","randomization_config":{"strategy":2,"block_size":0,"arms":[{"id":"d42d45ab-33ae-4d30-9910-678b32478f55","ratio":1,"locator":null},{"id":"7ad8afde-9190-4072-8406-7ff4bea61e50","ratio":1,"locator":null}],"keys":[],"strata":[]},"status":1,"organization":{"id":"ucsf","name":"University of California, San Francisco"},"created_by":{"id":"andrew","name":"Andrew Robinson","email":"andrew.robinson@ucsf.edu"},"created_at":1772550371,"updated_at":1772550371}"#;

        let study: LegacyStudy = serde_json::from_str(json).unwrap();

        assert_eq!(study.id, "bd7be9c5-708b-4b9b-b3cf-1d82c647ee16");
        assert_eq!(study.status, 1);
        assert_eq!(study.randomization_config.strategy, 2);
        assert_eq!(study.randomization_config.block_size, 0);
        assert_eq!(study.randomization_config.arms.len(), 2);
        assert_eq!(study.randomization_config.arms[0].ratio, 1);
        assert!(study.randomization_config.arms[0].locator.is_none());
        assert_eq!(study.created_at, 1772550371);
        assert_eq!(study.created_by.unwrap().id, "andrew");
    }

    #[test]
    fn deserializes_patient_id_key_variant() {
        let json = r#"[{"variant":{"PatientId":{}}}]"#;
        let keys: Vec<LegacyKey> = serde_json::from_str(json).unwrap();

        assert_eq!(keys.len(), 1);
        assert!(matches!(
            keys[0].variant,
            Some(LegacyKeyVariant::PatientId {})
        ));
    }

    #[test]
    fn deserializes_fhir_path_key_variant() {
        let json = r#"{"variant":{"FhirPath":{"id":"age","name":"Age","context":"Patient","path":"/age"}}}"#;
        let key: LegacyKey = serde_json::from_str(json).unwrap();

        match key.variant {
            Some(LegacyKeyVariant::FhirPath(fp)) => {
                assert_eq!(fp.id, "age");
                assert_eq!(fp.path, "/age");
            }
            other => panic!("expected FhirPath, got {other:?}"),
        }
    }

    #[test]
    fn deserializes_randomization() {
        let json = r#"{"id":"2f1df2e5-71a4-46b2-90a0-4bd22ac260bd","study_id":"228835de-96a7-442f-8fdb-6aeae6af8012","unit":{"kind":"simple","data":{"PatientId":"eaYQYiGc3dMeGffKe8qGUow3"}},"arm":{"id":"3dcce794-5284-4d7c-b773-2e7d18ffb589","ratio":1,"locator":5802},"issued_at":"2026-03-03T15:56:53.232231178Z"}"#;

        let rec: LegacyRandomization = serde_json::from_str(json).unwrap();

        assert_eq!(rec.id, "2f1df2e5-71a4-46b2-90a0-4bd22ac260bd");
        assert_eq!(rec.study_id, "228835de-96a7-442f-8fdb-6aeae6af8012");
        assert!(rec.strata.is_none());
        assert_eq!(rec.arm.locator, Some(5802));
        assert!(matches!(
            rec.unit,
            Unit::Simple(SimpleUnit::PatientId(ref id)) if id == "eaYQYiGc3dMeGffKe8qGUow3"
        ));
    }

    #[test]
    fn deserializes_block_preserving_values() {
        let json = r#"{"study_id":"228835de-96a7-442f-8fdb-6aeae6af8012","stratification":null,"index":0,"values":[0,1,1,0]}"#;
        let block: LegacyBlock = serde_json::from_str(json).unwrap();

        assert_eq!(block.index, 0);
        assert_eq!(block.values, vec![0, 1, 1, 0]);
        assert!(block.stratification.is_none());
    }

    #[test]
    fn deserializes_locator_mapping() {
        let json = r#"{"study_id":"228835de-96a7-442f-8fdb-6aeae6af8012"}"#;
        let mapping: LegacyLocatorMapping = serde_json::from_str(json).unwrap();

        assert_eq!(mapping.study_id, "228835de-96a7-442f-8fdb-6aeae6af8012");
    }
}
