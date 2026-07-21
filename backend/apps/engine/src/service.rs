//! Randomization service — orchestrates the full randomization flow.
//!
//! Responsibilities:
//! - Extract unit + strata from request context per study config
//! - Deduplicate concurrent requests for the same unit
//! - Load/create permuted blocks via the cache
//! - Record randomization assignments

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::Notify;
use tracing::{info, instrument};
use uuid::Uuid;

use pocr_core::environment::Environment;
use pocr_core::error::{RandomizationError, RandomizationResult};
use pocr_core::ports::{BlockCacheRepository, RandomizationRepository};
use pocr_core::random::{
    Arm, KeyVariant, RandomPermutedBlock, Randomization, RandomizationRequest, Strata, Unit,
};
use pocr_core::study::{gate, GateDecision, GateRejection, RandomizationStrategy, Study};

/// The randomization service wires together domain logic and persistence.
pub struct Engine {
    randomization_repo: Arc<dyn RandomizationRepository>,
    block_cache: Arc<dyn BlockCacheRepository>,
    /// In-flight deduplication: key → notify handle.
    inflight: DashMap<String, Arc<Notify>>,
}

/// Guard that ensures the inflight entry is removed and waiters notified
/// even if the randomization operation fails.
struct InflightGuard<'a> {
    map: &'a DashMap<String, Arc<Notify>>,
    key: String,
    notify: Arc<Notify>,
    defused: bool,
}

impl Drop for InflightGuard<'_> {
    fn drop(&mut self) {
        if !self.defused {
            self.map.remove(&self.key);
            self.notify.notify_waiters();
        }
    }
}

impl Engine {
    pub fn new(
        randomization_repo: Arc<dyn RandomizationRepository>,
        block_cache: Arc<dyn BlockCacheRepository>,
    ) -> Self {
        Self {
            randomization_repo,
            block_cache,
            inflight: DashMap::new(),
        }
    }

    /// Execute randomization for a study given a request in an environment.
    #[instrument(skip(self, study, request), fields(study_id = %study.id, environment = %environment))]
    pub async fn randomize(
        &self,
        study: &Study,
        environment: Environment,
        request: &RandomizationRequest,
    ) -> RandomizationResult<Randomization> {
        let config = &study.randomization_config;

        // Environment × status gate — must a request in this environment be
        // allowed to randomize into a study in this lifecycle state?
        if let GateDecision::Reject(reason) = gate(environment, study.status) {
            return Err(match reason {
                GateRejection::StudyNotActive => RandomizationError::StudyNotActive,
                GateRejection::StudyClosed => RandomizationError::StudyClosed,
                _ => RandomizationError::Unknown,
            });
        }

        let unit = extract_unit(&config.keys, request)?;
        let unit_key = String::from(&unit);

        if let Some(existing) = self
            .randomization_repo
            .exists_for_unit(&study.id, environment, &unit_key)
            .await
            .map_err(|e| RandomizationError::Infrastructure(e.to_string()))?
        {
            info!(unit = %unit_key, "existing randomization returned");
            return Ok(existing);
        }

        // Keyed by environment so a test request never blocks a production one.
        let dedup_key = format!("{}:{}:{}", environment.as_str(), study.id, unit_key);

        // Acquire the in-flight slot. Waiters block on the current holder's
        // notify; on wake they re-check idempotency and, if the holder failed
        // without persisting, contend again for the slot.
        let notify = loop {
            match self.inflight.entry(dedup_key.clone()) {
                dashmap::mapref::entry::Entry::Vacant(entry) => {
                    let notify = Arc::new(Notify::new());
                    entry.insert(notify.clone());
                    break notify;
                }
                dashmap::mapref::entry::Entry::Occupied(entry) => {
                    let notify = entry.get().clone();
                    drop(entry);
                    notify.notified().await;

                    if let Some(existing) = self
                        .randomization_repo
                        .exists_for_unit(&study.id, environment, &unit_key)
                        .await
                        .map_err(|e| RandomizationError::Infrastructure(e.to_string()))?
                    {
                        return Ok(existing);
                    }
                }
            }
        };

        let mut guard = InflightGuard {
            map: &self.inflight,
            key: dedup_key,
            notify: notify.clone(),
            defused: false,
        };

        let strata = if config.strata.is_empty() {
            None
        } else {
            Some(Strata::try_parse(&config.strata, request)?)
        };
        let stratum_key = strata.as_ref().map(String::from).unwrap_or_default();

        let arm_index = match config.strategy {
            RandomizationStrategy::Simple => Self::simple_randomize(&config.arms)?,
            RandomizationStrategy::Block => {
                self.block_randomize(
                    &study.id,
                    environment,
                    &config.arms,
                    config.block_size,
                    &stratum_key,
                )
                .await?
            }
        };

        let arm = config
            .arms
            .get(usize::try_from(arm_index).map_err(|_| RandomizationError::Unknown)?)
            .ok_or(RandomizationError::Unknown)?
            .clone();

        let epic_ids = request.extract_epic_ids();
        let mut randomization = match config.strategy {
            RandomizationStrategy::Simple => {
                Randomization::simple(&study.id, environment, &unit, &arm)
            }
            RandomizationStrategy::Block => {
                Randomization::block(&study.id, environment, strata.as_ref(), &unit, &arm)
            }
        };
        randomization.epic_patient_id = epic_ids.patient_id;
        randomization.epic_csn = epic_ids.csn;
        randomization.epic_user_id = epic_ids.user_id;

        let saved = self
            .randomization_repo
            .save(&randomization)
            .await
            .map_err(|e| RandomizationError::Infrastructure(e.to_string()))?;

        guard.defused = true;
        self.inflight.remove(&guard.key);
        notify.notify_waiters();

        info!(
            event_type = "randomization.issued",
            study_id = %study.id,
            arm_id = %saved.arm.id,
            unit = %unit_key,
            strategy = ?config.strategy,
            "randomization issued"
        );

        Ok(saved)
    }

    fn simple_randomize(arms: &[Arm]) -> RandomizationResult<u32> {
        use rand::distr::weighted::WeightedIndex;
        use rand::distr::Distribution;

        let weights: Vec<u32> = arms.iter().map(|a| a.ratio).collect();
        let dist = WeightedIndex::new(&weights).map_err(|_| {
            RandomizationError::InvalidConfiguration("all arm ratios are zero".into())
        })?;
        let mut rng = rand::rng();
        u32::try_from(dist.sample(&mut rng)).map_err(|_| RandomizationError::Unknown)
    }

    async fn block_randomize(
        &self,
        study_id: &Uuid,
        environment: Environment,
        arms: &[Arm],
        block_size: Option<u32>,
        stratum_key: &str,
    ) -> RandomizationResult<u32> {
        if let Some(arm_index) = self
            .block_cache
            .consume_next(environment, study_id, stratum_key)
            .await
            .map_err(|e| RandomizationError::Infrastructure(e.to_string()))?
        {
            return Ok(arm_index);
        }

        // Generate synchronously: ThreadRng is not Send and cannot cross await.
        let (arm_index, block) = {
            let mut rng = rand::rng();
            let strata = if stratum_key.is_empty() {
                None
            } else {
                use pocr_core::random::SimpleStratum;
                Some(Strata::Simple(SimpleStratum::FhirPath {
                    name: stratum_key.to_string(),
                    value: String::new(),
                }))
            };

            let mut block =
                RandomPermutedBlock::new(study_id, arms, block_size, strata.as_ref(), &mut rng)?;
            let arm_index = block.get_next()?;
            (arm_index, block)
        };

        self.block_cache
            .save_block(environment, &block)
            .await
            .map_err(|e| RandomizationError::Infrastructure(e.to_string()))?;

        Ok(arm_index)
    }
}

/// Extract the randomization unit from a request, defaulting to patient ID
/// when the study configures no keys.
fn extract_unit(keys: &[KeyVariant], request: &RandomizationRequest) -> RandomizationResult<Unit> {
    if keys.is_empty() {
        return Unit::try_parse(&[KeyVariant::PatientId], request);
    }
    Unit::try_parse(keys, request)
}

#[async_trait]
impl pocr_core::ports::RandomizationService for Engine {
    async fn randomize(
        &self,
        study: &Study,
        environment: Environment,
        request: &RandomizationRequest,
    ) -> RandomizationResult<Randomization> {
        self.randomize(study, environment, request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use pocr_core::ports::{BlockCacheRepository, RandomizationRepository, RepositoryError};
    use pocr_core::random::{Arm, RandomPermutedBlock, Randomization};
    use pocr_core::study::{RandomizationConfig, RandomizationStrategy, Study, StudyStatus};

    // -- In-memory test doubles --

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

        async fn exists_for_unit(
            &self,
            study_id: &Uuid,
            environment: Environment,
            unit_key: &str,
        ) -> Result<Option<Randomization>, RepositoryError> {
            let records = self.records.lock().unwrap();
            let found = records
                .iter()
                .find(|r| {
                    r.study_id == *study_id
                        && r.environment == environment
                        && String::from(&r.unit) == unit_key
                })
                .cloned();
            Ok(found)
        }

        async fn purge_nonproduction(&self, study_id: &Uuid) -> Result<u64, RepositoryError> {
            let mut records = self.records.lock().unwrap();
            let before = records.len();
            records.retain(|r| {
                !(r.study_id == *study_id && r.environment == Environment::NonProduction)
            });
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

    fn test_study(strategy: RandomizationStrategy) -> Study {
        Study {
            id: Uuid::now_v7(),
            name: "Test Study".to_string(),
            description: "A test study".to_string(),
            status: StudyStatus::Active,
            randomization_config: RandomizationConfig {
                strategy,
                arms: vec![
                    Arm {
                        id: Uuid::now_v7(),
                        name: "Control".to_string(),
                        ratio: 1,
                        locator: None,
                    },
                    Arm {
                        id: Uuid::now_v7(),
                        name: "Treatment".to_string(),
                        ratio: 1,
                        locator: None,
                    },
                ],
                keys: vec![KeyVariant::PatientId],
                strata: vec![],
                block_size: Some(4),
            },
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn test_request(patient_id: &str) -> RandomizationRequest {
        RandomizationRequest {
            locator: None,
            patient_id: Some(patient_id.to_string()),
            encounter_id: None,
            user_id: None,
            context: None,
        }
    }

    #[tokio::test]
    async fn randomize_simple_returns_assignment() {
        let repo = Arc::new(InMemoryRandomizationRepo::default());
        let cache = Arc::new(InMemoryBlockCache::default());
        let engine = Engine::new(repo.clone(), cache);

        let study = test_study(RandomizationStrategy::Simple);
        let request = test_request("P-001");

        let result = engine.randomize(&study, Environment::Production, &request).await;

        assert!(result.is_ok());
        let randomization = result.unwrap();
        assert_eq!(randomization.study_id, study.id);
    }

    #[tokio::test]
    async fn randomize_block_returns_assignment() {
        let repo = Arc::new(InMemoryRandomizationRepo::default());
        let cache = Arc::new(InMemoryBlockCache::default());
        let engine = Engine::new(repo.clone(), cache);

        let study = test_study(RandomizationStrategy::Block);
        let request = test_request("P-002");

        let result = engine.randomize(&study, Environment::Production, &request).await;

        assert!(result.is_ok());
        let randomization = result.unwrap();
        assert_eq!(randomization.study_id, study.id);
    }

    #[tokio::test]
    async fn randomize_is_idempotent_for_same_unit() {
        let repo = Arc::new(InMemoryRandomizationRepo::default());
        let cache = Arc::new(InMemoryBlockCache::default());
        let engine = Engine::new(repo.clone(), cache);

        let study = test_study(RandomizationStrategy::Simple);
        let request = test_request("P-003");

        let first = engine.randomize(&study, Environment::Production, &request).await.unwrap();
        let second = engine.randomize(&study, Environment::Production, &request).await.unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(first.arm.id, second.arm.id);

        // Only one record should be stored
        let records = repo.records.lock().unwrap();
        assert_eq!(records.len(), 1);
    }

    #[tokio::test]
    async fn randomize_different_patients_get_different_records() {
        let repo = Arc::new(InMemoryRandomizationRepo::default());
        let cache = Arc::new(InMemoryBlockCache::default());
        let engine = Engine::new(repo.clone(), cache);

        let study = test_study(RandomizationStrategy::Simple);

        let first = engine.randomize(&study, Environment::Production, &test_request("P-100")).await.unwrap();
        let second = engine.randomize(&study, Environment::Production, &test_request("P-200")).await.unwrap();

        assert_ne!(first.id, second.id);

        let records = repo.records.lock().unwrap();
        assert_eq!(records.len(), 2);
    }

    #[tokio::test]
    async fn randomize_missing_patient_id_returns_error() {
        let repo = Arc::new(InMemoryRandomizationRepo::default());
        let cache = Arc::new(InMemoryBlockCache::default());
        let engine = Engine::new(repo, cache);

        let study = test_study(RandomizationStrategy::Simple);
        let request = RandomizationRequest {
            locator: None,
            patient_id: None,
            encounter_id: None,
            user_id: None,
            context: None,
        };

        let result = engine.randomize(&study, Environment::Production, &request).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RandomizationError::MissingField { field: "patient_id" }
        ));
    }

    #[tokio::test]
    async fn block_randomize_saves_block_to_cache() {
        let repo = Arc::new(InMemoryRandomizationRepo::default());
        let cache = Arc::new(InMemoryBlockCache::default());
        let engine = Engine::new(repo, cache.clone());

        let study = test_study(RandomizationStrategy::Block);
        let request = test_request("P-004");

        engine.randomize(&study, Environment::Production, &request).await.unwrap();

        // Verify block was saved to cache
        let blocks = cache.blocks.lock().unwrap();
        assert!(!blocks.is_empty());
    }

    // ── Environment isolation ────────────────────────────────────────────

    #[tokio::test]
    async fn same_unit_randomizes_independently_per_environment() {
        let repo = Arc::new(InMemoryRandomizationRepo::default());
        let cache = Arc::new(InMemoryBlockCache::default());
        let engine = Engine::new(repo.clone(), cache);

        let study = test_study(RandomizationStrategy::Simple);
        let request = test_request("P-SHARED");

        let prod = engine
            .randomize(&study, Environment::Production, &request)
            .await
            .unwrap();
        let test = engine
            .randomize(&study, Environment::NonProduction, &request)
            .await
            .unwrap();

        // Two distinct records — the non-production request did not dedup
        // against the production one.
        assert_ne!(prod.id, test.id);
        assert_eq!(prod.environment, Environment::Production);
        assert_eq!(test.environment, Environment::NonProduction);

        let records = repo.records.lock().unwrap();
        assert_eq!(records.len(), 2);
    }

    #[tokio::test]
    async fn block_cache_keys_differ_by_environment() {
        let repo = Arc::new(InMemoryRandomizationRepo::default());
        let cache = Arc::new(InMemoryBlockCache::default());
        let engine = Engine::new(repo, cache.clone());

        let study = test_study(RandomizationStrategy::Block);

        engine
            .randomize(&study, Environment::Production, &test_request("P-A"))
            .await
            .unwrap();
        engine
            .randomize(&study, Environment::NonProduction, &test_request("P-B"))
            .await
            .unwrap();

        let blocks = cache.blocks.lock().unwrap();
        // One key per environment — the sequences are independent.
        assert_eq!(blocks.len(), 2);
        assert!(blocks.keys().any(|k| k.starts_with("production:")));
        assert!(blocks.keys().any(|k| k.starts_with("nonproduction:")));
    }

    // ── Environment × status gating ──────────────────────────────────────

    fn study_with_status(status: StudyStatus) -> Study {
        let mut s = test_study(RandomizationStrategy::Simple);
        s.status = status;
        s
    }

    #[tokio::test]
    async fn production_rejected_when_study_not_active() {
        let engine = Engine::new(
            Arc::new(InMemoryRandomizationRepo::default()),
            Arc::new(InMemoryBlockCache::default()),
        );
        let study = study_with_status(StudyStatus::Draft);

        let err = engine
            .randomize(&study, Environment::Production, &test_request("P-1"))
            .await
            .unwrap_err();
        assert!(matches!(err, RandomizationError::StudyNotActive));
    }

    #[tokio::test]
    async fn nonproduction_allowed_when_study_draft() {
        let engine = Engine::new(
            Arc::new(InMemoryRandomizationRepo::default()),
            Arc::new(InMemoryBlockCache::default()),
        );
        let study = study_with_status(StudyStatus::Draft);

        let result = engine
            .randomize(&study, Environment::NonProduction, &test_request("P-1"))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn closed_study_rejects_all_environments() {
        let engine = Engine::new(
            Arc::new(InMemoryRandomizationRepo::default()),
            Arc::new(InMemoryBlockCache::default()),
        );
        let study = study_with_status(StudyStatus::Closed);

        for env in [Environment::Production, Environment::NonProduction] {
            let err = engine
                .randomize(&study, env, &test_request("P-1"))
                .await
                .unwrap_err();
            assert!(matches!(err, RandomizationError::StudyClosed));
        }
    }
}
