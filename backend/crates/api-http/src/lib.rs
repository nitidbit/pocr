//! HTTP API routes and handlers for the POCR engine.
//!
//! [`app`] assembles the full [`Router`] over an [`AppState`]; [`ApiError`]
//! maps handler failures to JSON error responses.

mod balance;
mod cds_hooks;
mod datasets;
mod error;
mod extractors;
mod health;
mod me;
mod members;
mod randomizations;
mod sharing;
mod smd;
mod studies;
mod system;

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::extract::FromRef;
use axum::routing::{get, post};
use tower_http::trace::TraceLayer;

use pocr_core::environment::EnvironmentMap;
use pocr_core::ports::{
    AuditRepository, BlockCacheRepository, DemographicsRepository, HealthCheckable,
    LocatorRepository, RandomizationRepository, RandomizationService, StudyCache, StudyRepository,
    UserStudyRepository,
};

pub use datasets::{DatasetsState, router as datasets_router};
pub use error::ApiError;

use crate::balance::get_balance;
use crate::randomizations::{count_randomizations, list_randomizations};
use crate::studies::{create_study, get_study, list_studies};

/// Shared application state passed to all handlers.
///
/// Cloned per request, so every field is cheap to clone.
#[derive(Clone)]
pub struct AppState {
    /// Persistent study storage.
    pub study_repo: Arc<dyn StudyRepository>,
    /// Persistent randomization records.
    pub randomization_repo: Arc<dyn RandomizationRepository>,
    /// Cache of generated permuted blocks.
    pub block_cache: Arc<dyn BlockCacheRepository>,
    /// Study membership storage.
    pub user_study_repo: Arc<dyn UserStudyRepository>,
    /// Append-only audit trail of study mutations.
    pub audit_repo: Arc<dyn AuditRepository>,
    /// Mapping from Epic locator IDs to studies.
    pub locator_repo: Arc<dyn LocatorRepository>,
    /// Executes randomization requests against a study.
    pub randomization_service: Arc<dyn RandomizationService>,
    /// Synced patient demographics read by the balance endpoint.
    pub demographics_repo: Arc<dyn DemographicsRepository>,
    /// Cache of study bodies and locator mappings.
    pub study_cache: Arc<dyn StudyCache>,
    /// Maps CDS Hooks `fhirServer` URL prefixes to deployment environments.
    ///
    /// Resolution fails closed on unknown or missing URLs.
    pub environment_map: Arc<EnvironmentMap>,
    /// PostgreSQL connectivity probe for health checks.
    pub pg_health: Arc<dyn HealthCheckable>,
    /// Redis connectivity probe for health checks.
    pub redis_health: Arc<dyn HealthCheckable>,
    /// JWT secret for study sharing tokens.
    pub jwt_secret: Arc<str>,
    /// Path to the TLS certificate PEM file, if one is configured.
    pub cert_path: Option<Arc<Path>>,
    /// Dependencies of the dataset routes.
    pub datasets: datasets::DatasetsState,
    /// Server start time, used to report uptime.
    pub started_at: std::time::Instant,
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

impl FromRef<AppState> for datasets::DatasetsState {
    fn from_ref(input: &AppState) -> Self {
        input.datasets.clone()
    }
}

/// Builds the full application router with shared state.
pub fn app(state: AppState) -> Router {
    let api_v1 = Router::new()
        .route("/studies", get(list_studies).post(create_study))
        .route(
            "/studies/{id}",
            get(get_study)
                .put(studies::update_study)
                .delete(studies::delete_study),
        )
        .route("/studies/{id}/transition", post(studies::transition_study))
        .route(
            "/studies/{id}/nonproduction",
            axum::routing::delete(studies::purge_nonproduction),
        )
        .route(
            "/studies/{id}/arms/{arm_id}/locator",
            axum::routing::put(studies::set_arm_locator),
        )
        .route(
            "/studies/{id}/arms/ratios",
            axum::routing::put(studies::update_ratios),
        )
        .route(
            "/studies/{id}/members",
            get(members::list).post(members::add),
        )
        .route(
            "/studies/{id}/members/{email}",
            axum::routing::put(members::update_role).delete(members::remove),
        )
        .route("/studies/{id}/share", post(sharing::share_study))
        .route("/studies/join", post(sharing::join_study))
        .route("/studies/{id}/audit", get(studies::list_audit))
        .route(
            "/studies/{study_id}/randomizations",
            get(list_randomizations),
        )
        .route(
            "/studies/{study_id}/randomizations/counts",
            get(count_randomizations),
        )
        .route("/studies/{study_id}/balance", get(get_balance))
        .route("/system/status", get(system::get_system_status))
        .merge(datasets::router());

    let cds = Router::new()
        .route("/cds-services", get(cds_hooks::discover_services))
        .route("/cds-services/randomization", post(cds_hooks::randomize));

    Router::new()
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready))
        .route("/api/me", get(me::get_me))
        .nest("/api/v1", api_v1)
        .merge(cds)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
