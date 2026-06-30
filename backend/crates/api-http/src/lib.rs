//! HTTP API routes and handlers for the POCR engine.

pub mod cds_hooks;
pub mod error;
pub mod extractors;
mod health;
pub mod randomizations;
pub mod sharing;
pub mod smd;
pub mod studies;

use std::pin::Pin;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use pocr_core::ports::{
    LocatorRepository, RandomizationRepository, StudyRepository, UserStudyRepository,
};

use crate::randomizations::list_randomizations;
use crate::studies::{create_study, get_study, list_studies};

/// Shared application state passed to all handlers.
pub struct AppState {
    pub study_repo: Arc<dyn StudyRepository>,
    pub randomization_repo: Arc<dyn RandomizationRepository>,
    pub user_study_repo: Arc<dyn UserStudyRepository>,
    pub locator_repo: Arc<dyn LocatorRepository>,
    pub randomization_service: Arc<dyn RandomizationService>,
    /// PostgreSQL pool for health checks.
    pub pg_pool: sqlx::PgPool,
    /// Redis pool for health checks.
    pub redis_pool: Arc<dyn HealthCheckable>,
    /// JWT secret for study sharing tokens.
    pub jwt_secret: String,
}

/// Trait for checking if a service is healthy.
pub trait HealthCheckable: Send + Sync {
    fn ping(&self) -> Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;
}

/// Trait for the randomization service orchestrator.
pub trait RandomizationService: Send + Sync {
    fn randomize<'a>(
        &'a self,
        study: &'a pocr_core::study::Study,
        request: &'a pocr_core::random::RandomizationRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<pocr_core::random::Randomization, pocr_core::error::RandomizationError>> + Send + 'a>>;
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
        .route(
            "/studies/{id}/arms/{arm_id}/locator",
            axum::routing::put(studies::set_arm_locator),
        )
        .route("/studies/{id}/share", post(sharing::share_study))
        .route("/studies/join", post(sharing::join_study))
        .route("/studies/{study_id}/randomizations", get(list_randomizations));

    let cds = Router::new()
        .route("/cds-services", get(cds_hooks::discover_services))
        .route(
            "/cds-services/randomization",
            post(cds_hooks::randomize),
        );

    Router::new()
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready))
        .nest("/api/v1", api_v1)
        .merge(cds)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
