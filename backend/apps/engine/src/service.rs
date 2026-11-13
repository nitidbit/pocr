//! Randomization service that orchestrates the full randomization flow.
//!
//! Responsibilities:
//! - Extracts the unit and strata from the request context per study config
//! - Deduplicates concurrent requests for the same unit
//! - Loads or creates permuted blocks via the cache
//! - Records randomization assignments

use std::fmt;
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
use pocr_core::study::{GateDecision, GateRejection, RandomizationStrategy, Study, gate};

/// Wires together randomization domain logic and persistence.
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

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("inflight", &self.inflight.len())
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// Creates an engine that persists assignments to `randomization_repo` and
    /// draws permuted blocks from `block_cache`.
    #[must_use]
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

    /// Randomizes the request's unit into an arm of `study` within `environment`.
    ///
    /// Returns the existing assignment when the unit was already randomized.
    /// Concurrent requests for the same unit wait for the first to finish.
    ///
    /// # Errors
    ///
    /// Returns a [`RandomizationError`] if the environment/status gate rejects
    /// the request, the request lacks a configured key or stratum, the study
    /// configuration cannot be sampled, or a repository call fails.
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
            .await?
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
                    entry.insert(Arc::clone(&notify));
                    break notify;
                }
                dashmap::mapref::entry::Entry::Occupied(entry) => {
                    let notify = Arc::clone(entry.get());
                    // enable() registers this waiter before the map guard is
                    // released, so a notify from the holder in the gap between
                    // dropping the guard and awaiting cannot be lost.
                    let notified = notify.notified();
                    tokio::pin!(notified);
                    notified.as_mut().enable();
                    drop(entry);
                    notified.await;

                    if let Some(existing) = self
                        .randomization_repo
                        .exists_for_unit(&study.id, environment, &unit_key)
                        .await?
                    {
                        return Ok(existing);
                    }
                }
            }
        };

        let mut guard = InflightGuard {
            map: &self.inflight,
            key: dedup_key,
            notify: Arc::clone(&notify),
            defused: false,
        };

        let strata = if config.strata.is_empty() {
            None
        } else {
            Some(Strata::try_parse(&config.strata, request)?)
        };
        let stratum_key = strata.as_ref().map_or_else(String::new, String::from);

        let arm = self.pick_arm(study, environment, &stratum_key).await?;

        let mut randomization = match config.strategy {
            RandomizationStrategy::Simple => {
                Randomization::simple(&study.id, environment, &unit, &arm)
            }
            RandomizationStrategy::Block => {
                Randomization::block(&study.id, environment, strata.as_ref(), &unit, &arm)
            }
        };
        randomization.context = request.extract_context();

        let saved = self.randomization_repo.save(&randomization).await?;

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

    async fn pick_arm(
        &self,
        study: &Study,
        environment: Environment,
        stratum_key: &str,
    ) -> RandomizationResult<Arm> {
        let config = &study.randomization_config;
        let arm_index = match config.strategy {
            RandomizationStrategy::Simple => Self::simple_randomize(&config.arms)?,
            RandomizationStrategy::Block => {
                self.block_randomize(
                    &study.id,
                    environment,
                    &config.arms,
                    config.block_size,
                    stratum_key,
                )
                .await?
            }
        };

        #[expect(
            clippy::map_err_ignore,
            reason = "the failed conversion carries no cause beyond the out-of-range index"
        )]
        let arm_slot = usize::try_from(arm_index).map_err(|_| RandomizationError::Unknown)?;
        config
            .arms
            .get(arm_slot)
            .cloned()
            .ok_or(RandomizationError::Unknown)
    }

    fn simple_randomize(arms: &[Arm]) -> RandomizationResult<u32> {
        use rand::distr::Distribution;
        use rand::distr::weighted::WeightedIndex;

        let weights: Vec<u32> = arms.iter().map(|a| a.ratio).collect();
        #[expect(
            clippy::map_err_ignore,
            reason = "only zero weights reach here; the config error is the full diagnosis"
        )]
        let dist = WeightedIndex::new(&weights).map_err(|_| {
            RandomizationError::InvalidConfiguration("all arm ratios are zero".into())
        })?;
        let mut rng = rand::rng();
        #[expect(
            clippy::map_err_ignore,
            reason = "the failed conversion carries no cause beyond the out-of-range index"
        )]
        let index =
            u32::try_from(dist.sample(&mut rng)).map_err(|_| RandomizationError::Unknown)?;
        Ok(index)
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
            .await?
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

        self.block_cache.save_block(environment, &block).await?;

        Ok(arm_index)
    }
}

/// Extracts the randomization unit from a request, defaulting to patient ID
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
