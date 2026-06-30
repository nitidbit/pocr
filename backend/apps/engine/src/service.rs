//! Randomization service — orchestrates the full randomization flow.
//!
//! Responsibilities:
//! - Extract unit + strata from request context per study config
//! - Deduplicate concurrent requests for the same unit
//! - Load/create permuted blocks via the cache
//! - Record randomization assignments

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::Notify;
use tracing::{info, instrument};
use uuid::Uuid;

use pocr_core::error::{RandomizationError, RandomizationResult};
use pocr_core::ports::{BlockCacheRepository, RandomizationRepository};
use pocr_core::random::{
    Arm, Key, KeyVariant, RandomPermutedBlock, Randomization, RandomizationRequest, Strata, Unit,
};
use pocr_core::study::{RandomizationStrategy, Study};

/// The randomization service wires together domain logic and persistence.
pub struct Engine {
    randomization_repo: Arc<dyn RandomizationRepository>,
    block_cache: Arc<dyn BlockCacheRepository>,
    /// In-flight deduplication: key → notify handle.
    inflight: DashMap<String, Arc<Notify>>,
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

    /// Execute randomization for a study given a request.
    #[instrument(skip(self, study, request), fields(study_id = %study.id))]
    pub async fn randomize(
        &self,
        study: &Study,
        request: &RandomizationRequest,
    ) -> RandomizationResult<Randomization> {
        let config = &study.randomization_config;

        // 1. Extract unit from request using study keys
        let unit = self.extract_unit(&config.keys, request)?;
        let unit_hash = String::from(&unit);

        // 2. Idempotency check — return existing if already randomized
        if let Some(existing) = self
            .randomization_repo
            .exists_for_unit(&study.id, &unit_hash)
            .await
            .map_err(|e| RandomizationError::NotImplemented(e.to_string()))?
        {
            info!(unit = %unit_hash, "existing randomization returned");
            return Ok(existing);
        }

        // 3. Deduplication — coalesce concurrent requests for the same unit
        let dedup_key = format!("{}:{}", study.id, unit_hash);
        if let Some(entry) = self.inflight.get(&dedup_key) {
            let notify = entry.value().clone();
            drop(entry); // Release DashMap Ref guard before awaiting
            notify.notified().await;
            // Re-check idempotency after wake
            if let Some(existing) = self
                .randomization_repo
                .exists_for_unit(&study.id, &unit_hash)
                .await
                .map_err(|e| RandomizationError::NotImplemented(e.to_string()))?
            {
                return Ok(existing);
            }
        }

        // Register this request as in-flight
        let notify = Arc::new(Notify::new());
        self.inflight.insert(dedup_key.clone(), notify.clone());

        // 4. Extract strata (if configured)
        let strata = if config.strata.is_empty() {
            None
        } else {
            Some(Strata::try_parse(&config.strata, request)?)
        };
        let stratum_key = strata.as_ref().map(String::from).unwrap_or_default();

        // 5. Perform randomization based on strategy
        let arm_index = match config.strategy {
            RandomizationStrategy::Simple => self.simple_randomize(&config.arms),
            RandomizationStrategy::Block => {
                self.block_randomize(&study.id, &config.arms, config.block_size, &stratum_key)
                    .await?
            }
        };

        let arm = config
            .arms
            .get(arm_index as usize)
            .ok_or(RandomizationError::Unknown)?
            .clone();

        // 6. Create and persist the randomization record
        let epic_ids = request.extract_epic_ids();
        let mut randomization = match config.strategy {
            RandomizationStrategy::Simple => {
                Randomization::simple(&study.id, &unit, &arm)
            }
            RandomizationStrategy::Block => {
                Randomization::block(&study.id, strata.as_ref(), &unit, &arm)
            }
        };
        randomization.epic_patient_id = epic_ids.patient_id;
        randomization.csn = epic_ids.csn;
        randomization.epic_provider_id = epic_ids.provider_id;

        let saved = self
            .randomization_repo
            .save(&randomization)
            .await
            .map_err(|e| RandomizationError::NotImplemented(e.to_string()))?;

        // 7. Clean up deduplication and notify waiters
        self.inflight.remove(&dedup_key);
        notify.notify_waiters();

        info!(
            event_type = "randomization.issued",
            study_id = %study.id,
            arm_id = %saved.arm.id,
            unit = %unit_hash,
            strategy = ?config.strategy,
            "randomization issued"
        );

        Ok(saved)
    }

    fn extract_unit(
        &self,
        keys: &[Key],
        request: &RandomizationRequest,
    ) -> RandomizationResult<Unit> {
        if keys.is_empty() {
            // Default to patient ID if no keys configured
            let default_keys = vec![Some(KeyVariant::PatientId)];
            return Unit::try_parse(&default_keys, request);
        }
        Unit::try_parse(keys, request)
    }

    fn simple_randomize(&self, arms: &[Arm]) -> u32 {
        use rand::distr::weighted::WeightedIndex;
        use rand::distr::Distribution;

        let weights: Vec<u32> = arms.iter().map(|a| a.ratio).collect();
        let dist = WeightedIndex::new(&weights).expect("arm ratios must be non-zero");
        let mut rng = rand::rng();
        dist.sample(&mut rng) as u32
    }

    async fn block_randomize(
        &self,
        study_id: &Uuid,
        arms: &[Arm],
        block_size: Option<u32>,
        stratum_key: &str,
    ) -> RandomizationResult<u32> {
        // Try to consume from existing cached block
        if let Some(arm_index) = self
            .block_cache
            .consume_next(study_id, stratum_key)
            .await
            .map_err(|e| RandomizationError::NotImplemented(e.to_string()))?
        {
            return Ok(arm_index);
        }

        // Block exhausted or not found — generate a new one.
        // Generate block synchronously (ThreadRng is not Send).
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

        // Save the rest of the block to cache
        self.block_cache
            .save_block(&block)
            .await
            .map_err(|e| RandomizationError::NotImplemented(e.to_string()))?;

        Ok(arm_index)
    }
}

impl api_http::RandomizationService for Engine {
    fn randomize<'a>(
        &'a self,
        study: &'a Study,
        request: &'a RandomizationRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = RandomizationResult<Randomization>> + Send + 'a>> {
        Box::pin(self.randomize(study, request))
    }
}
