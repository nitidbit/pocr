//! Public-API tests of [`Engine::randomize`] against in-memory repositories.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pocr_core::environment::Environment;
use pocr_core::error::RandomizationError;
use pocr_core::ports::{ArmTally, BlockCacheRepository, RandomizationRepository, RepositoryError};
use pocr_core::random::{
    Arm, KeyVariant, RandomPermutedBlock, Randomization, RandomizationRequest,
};
use pocr_core::study::{RandomizationConfig, RandomizationStrategy, Study, StudyStatus};
use pocr_engine::Engine;
use uuid::Uuid;

#[derive(Default)]
struct InMemoryRandomizationRepo {
    records: Mutex<Vec<Randomization>>,
}

#[async_trait]
impl RandomizationRepository for InMemoryRandomizationRepo {
    async fn save(&self, randomization: &Randomization) -> Result<Randomization, RepositoryError> {
        let mut records = self.records.lock().unwrap();
        records.push(randomization.clone());
        Ok(randomization.clone())
    }

    async fn find_by_study(
        &self,
        study_id: &Uuid,
        environment: Environment,
        _limit: u32,
        _offset: u32,
    ) -> Result<Vec<Randomization>, RepositoryError> {
        let records = self.records.lock().unwrap();
        Ok(records
            .iter()
            .filter(|r| r.study_id == *study_id && r.environment == environment)
            .cloned()
            .collect())
    }

    async fn count_by_arm(
        &self,
        study_id: &Uuid,
        environment: Environment,
    ) -> Result<Vec<ArmTally>, RepositoryError> {
        let records = self.records.lock().unwrap();
        let mut counts: HashMap<Uuid, u64> = HashMap::new();
        for r in records
            .iter()
            .filter(|r| r.study_id == *study_id && r.environment == environment)
        {
            *counts.entry(r.arm.id).or_default() += 1;
        }
        Ok(counts
            .into_iter()
            .map(|(arm_id, count)| ArmTally { arm_id, count })
            .collect())
    }

    async fn exists_for_unit(
        &self,
        study_id: &Uuid,
        environment: Environment,
        unit_key: &str,
    ) -> Result<Option<Randomization>, RepositoryError> {
        let records = self.records.lock().unwrap();
        Ok(records
            .iter()
            .find(|r| {
                r.study_id == *study_id
                    && r.environment == environment
                    && String::from(&r.unit) == unit_key
            })
            .cloned())
    }

    async fn purge_nonproduction(&self, study_id: &Uuid) -> Result<u64, RepositoryError> {
        let mut records = self.records.lock().unwrap();
        let before = records.len();
        records
            .retain(|r| !(r.study_id == *study_id && r.environment == Environment::NonProduction));
        Ok((before - records.len()) as u64)
    }
}

#[derive(Default)]
struct InMemoryBlockCache {
    blocks: Mutex<HashMap<String, Vec<u32>>>,
}

#[async_trait]
impl BlockCacheRepository for InMemoryBlockCache {
    async fn get_block(
        &self,
        _environment: Environment,
        _study_id: &Uuid,
        _stratum_key: &str,
    ) -> Result<Option<RandomPermutedBlock>, RepositoryError> {
        Ok(None)
    }

    async fn save_block(
        &self,
        environment: Environment,
        block: &RandomPermutedBlock,
    ) -> Result<(), RepositoryError> {
        let key = format!(
            "{}:{}:{}",
            environment.as_str(),
            block.study_id,
            block.stratification.as_deref().unwrap_or("")
        );
        self.blocks.lock().unwrap().insert(key, vec![]);
        Ok(())
    }

    async fn consume_next(
        &self,
        _environment: Environment,
        _study_id: &Uuid,
        _stratum_key: &str,
    ) -> Result<Option<u32>, RepositoryError> {
        Ok(None)
    }

    async fn purge_nonproduction(&self, study_id: &Uuid) -> Result<u64, RepositoryError> {
        let mut blocks = self.blocks.lock().unwrap();
        let prefix = format!("{}:{}:", Environment::NonProduction.as_str(), study_id);
        let before = blocks.len();
        blocks.retain(|k, _| !k.starts_with(&prefix));
        Ok((before - blocks.len()) as u64)
    }
}

struct Fixture {
    engine: Engine,
    repo: Arc<InMemoryRandomizationRepo>,
    cache: Arc<InMemoryBlockCache>,
}

fn fixture() -> Fixture {
    let repo = Arc::new(InMemoryRandomizationRepo::default());
    let cache = Arc::new(InMemoryBlockCache::default());
    let engine = Engine::new(Arc::clone(&repo) as _, Arc::clone(&cache) as _);
    Fixture {
        engine,
        repo,
        cache,
    }
}

fn test_study(strategy: RandomizationStrategy) -> Study {
    let arm = |name: &str| Arm {
        id: Uuid::now_v7(),
        name: name.to_string(),
        ratio: 1,
        locator: None,
    };
    Study {
        id: Uuid::now_v7(),
        name: "Test Study".to_string(),
        description: "A test study".to_string(),
        status: StudyStatus::Active,
        randomization_config: RandomizationConfig {
            strategy,
            arms: vec![arm("Control"), arm("Treatment")],
            keys: vec![KeyVariant::PatientId],
            strata: vec![],
            block_size: Some(4),
        },
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn study_with_status(status: StudyStatus) -> Study {
    let mut study = test_study(RandomizationStrategy::Simple);
    study.status = status;
    study
}

fn test_request(patient_id: &str) -> RandomizationRequest {
    RandomizationRequest {
        locator: None,
        patient_id: Some(patient_id.to_string()),
        encounter_id: None,
        user_id: None,
        fhir_server: None,
        context: None,
    }
}

fn request_with_context() -> RandomizationRequest {
    let mut request = test_request("fhir-patient");
    request.encounter_id = Some("fhir-encounter".into());
    request.user_id = Some("Practitioner/fhir-user".into());
    request.fhir_server = Some("https://synthetic.example/FHIR/R4".into());
    request.context = Some(serde_json::json!({
        "patient": {"identifier": [{"type": {"text": "EXTERNAL"}, "value": "000123"}]},
        "encounter": {"identifier": [{"system": "urn:oid:1.7.3.698084.8", "value": "000456"}]},
        "unrelated": {"secret": "never persist entire prefetch"}
    }));
    request
}

async fn context_of(
    engine: &Engine,
    study: &Study,
    request: &RandomizationRequest,
) -> serde_json::Value {
    let randomization = engine
        .randomize(study, Environment::Production, request)
        .await
        .unwrap();
    serde_json::to_value(randomization).unwrap()["context"].clone()
}

#[tokio::test]
async fn context_captures_request_and_prefetch_identifiers() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Simple);

    let context = context_of(&f.engine, &study, &request_with_context()).await;

    assert_eq!(context["fhir_patient_id"], "fhir-patient");
    assert_eq!(context["fhir_encounter_id"], "fhir-encounter");
    assert_eq!(context["fhir_user_reference"], "Practitioner/fhir-user");
    assert_eq!(context["epic_patient_id"], "000123");
    assert_eq!(context["epic_csn"], "000456");
    assert_eq!(context["fhir_server"], "https://synthetic.example/FHIR/R4");
}

#[tokio::test]
async fn context_omits_unrelated_prefetch_data() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Simple);

    let context = context_of(&f.engine, &study, &request_with_context()).await;

    assert!(context.get("unrelated").is_none());
    assert!(context.get("epic_user_id").is_none());
}

#[tokio::test]
async fn replay_keeps_originally_captured_context() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Simple);
    let mut request = request_with_context();
    let first = context_of(&f.engine, &study, &request).await;

    request.encounter_id = Some("later-encounter".into());
    let replay = context_of(&f.engine, &study, &request).await;

    assert_eq!(first, replay);
}

#[tokio::test]
async fn simple_strategy_returns_assignment_for_study() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Simple);

    let randomization = f
        .engine
        .randomize(&study, Environment::Production, &test_request("P-001"))
        .await
        .unwrap();

    assert_eq!(randomization.study_id, study.id);
}

#[tokio::test]
async fn block_strategy_returns_assignment_for_study() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Block);

    let randomization = f
        .engine
        .randomize(&study, Environment::Production, &test_request("P-002"))
        .await
        .unwrap();

    assert_eq!(randomization.study_id, study.id);
}

#[tokio::test]
async fn repeat_request_returns_same_randomization() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Simple);
    let request = test_request("P-003");

    let first = f
        .engine
        .randomize(&study, Environment::Production, &request)
        .await
        .unwrap();
    let second = f
        .engine
        .randomize(&study, Environment::Production, &request)
        .await
        .unwrap();

    assert_eq!(first.id, second.id);
    assert_eq!(first.arm.id, second.arm.id);
}

#[tokio::test]
async fn repeat_request_stores_single_record() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Simple);
    let request = test_request("P-003");

    for _ in 0..2 {
        f.engine
            .randomize(&study, Environment::Production, &request)
            .await
            .unwrap();
    }

    assert_eq!(f.repo.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn different_patients_get_distinct_records() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Simple);

    let first = f
        .engine
        .randomize(&study, Environment::Production, &test_request("P-100"))
        .await
        .unwrap();
    let second = f
        .engine
        .randomize(&study, Environment::Production, &test_request("P-200"))
        .await
        .unwrap();

    assert_ne!(first.id, second.id);
    assert_eq!(f.repo.records.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn missing_patient_id_is_rejected() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Simple);
    let request = RandomizationRequest {
        patient_id: None,
        ..test_request("unused")
    };

    let err = f
        .engine
        .randomize(&study, Environment::Production, &request)
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        RandomizationError::MissingField {
            field: "patient_id"
        }
    ));
}

#[tokio::test]
async fn block_strategy_saves_block_to_cache() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Block);

    f.engine
        .randomize(&study, Environment::Production, &test_request("P-004"))
        .await
        .unwrap();

    assert!(!f.cache.blocks.lock().unwrap().is_empty());
}

#[tokio::test]
async fn same_unit_gets_distinct_records_per_environment() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Simple);
    let request = test_request("P-SHARED");

    let prod = f
        .engine
        .randomize(&study, Environment::Production, &request)
        .await
        .unwrap();
    let nonprod = f
        .engine
        .randomize(&study, Environment::NonProduction, &request)
        .await
        .unwrap();

    assert_ne!(prod.id, nonprod.id);
    assert_eq!(prod.environment, Environment::Production);
    assert_eq!(nonprod.environment, Environment::NonProduction);
}

#[tokio::test]
async fn block_cache_keys_differ_by_environment() {
    let f = fixture();
    let study = test_study(RandomizationStrategy::Block);

    f.engine
        .randomize(&study, Environment::Production, &test_request("P-A"))
        .await
        .unwrap();
    f.engine
        .randomize(&study, Environment::NonProduction, &test_request("P-B"))
        .await
        .unwrap();

    let blocks = f.cache.blocks.lock().unwrap();
    assert_eq!(blocks.len(), 2);
    assert!(blocks.keys().any(|k| k.starts_with("production:")));
    assert!(blocks.keys().any(|k| k.starts_with("nonproduction:")));
}

#[tokio::test]
async fn production_rejected_when_study_not_active() {
    let f = fixture();
    let study = study_with_status(StudyStatus::Draft);

    let err = f
        .engine
        .randomize(&study, Environment::Production, &test_request("P-1"))
        .await
        .unwrap_err();

    assert!(matches!(err, RandomizationError::StudyNotActive));
}

#[tokio::test]
async fn nonproduction_allowed_when_study_draft() {
    let f = fixture();
    let study = study_with_status(StudyStatus::Draft);

    let randomization = f
        .engine
        .randomize(&study, Environment::NonProduction, &test_request("P-1"))
        .await
        .unwrap();

    assert_eq!(randomization.environment, Environment::NonProduction);
}

#[tokio::test]
async fn closed_study_rejects_all_environments() {
    let f = fixture();
    let study = study_with_status(StudyStatus::Closed);

    for env in [Environment::Production, Environment::NonProduction] {
        let err = f
            .engine
            .randomize(&study, env, &test_request("P-1"))
            .await
            .unwrap_err();
        assert!(matches!(err, RandomizationError::StudyClosed));
    }
}
