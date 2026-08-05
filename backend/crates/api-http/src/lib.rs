//! HTTP API routes and handlers for the POCR engine.

pub mod balance;
pub mod cds_hooks;
pub mod error;
pub mod extractors;
mod health;
pub mod members;
pub mod randomizations;
pub mod sharing;
pub mod smd;
pub mod studies;
pub mod system;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};
use tower_http::trace::TraceLayer;

use pocr_core::environment::EnvironmentMap;
use pocr_core::ports::{
    AuditRepository, BlockCacheRepository, DemographicsRepository, HealthCheckable,
    LocatorRepository, RandomizationRepository, RandomizationService, StudyCache, StudyRepository,
    UserStudyRepository,
};

use crate::balance::get_balance;
use crate::randomizations::{count_randomizations, list_randomizations};
use crate::studies::{create_study, get_study, list_studies};
pub mod me;

/// Shared application state passed to all handlers.
pub struct AppState {
    pub study_repo: Arc<dyn StudyRepository>,
    pub randomization_repo: Arc<dyn RandomizationRepository>,
    pub block_cache: Arc<dyn BlockCacheRepository>,
    pub user_study_repo: Arc<dyn UserStudyRepository>,
    pub audit_repo: Arc<dyn AuditRepository>,
    pub locator_repo: Arc<dyn LocatorRepository>,
    pub randomization_service: Arc<dyn RandomizationService>,
    pub demographics_repo: Arc<dyn DemographicsRepository>,
    pub study_cache: Arc<dyn StudyCache>,
    /// Maps CDS Hooks `fhirServer` URL prefixes to deployment environments.
    /// Resolution fails closed on unknown/missing URLs.
    pub environment_map: EnvironmentMap,
    /// PostgreSQL connectivity probe for health checks.
    pub pg_health: Arc<dyn HealthCheckable>,
    /// Redis connectivity probe for health checks.
    pub redis_health: Arc<dyn HealthCheckable>,
    /// JWT secret for study sharing tokens.
    pub jwt_secret: String,
    /// Path to TLS certificate PEM file (optional).
    pub cert_path: Option<String>,
    /// Server start time for uptime calculation.
    pub started_at: std::time::Instant,
}

/// Build the full application router with shared state.
pub fn app(state: Arc<AppState>) -> Router {
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
        .route("/system/status", get(system::get_system_status));

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
