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
use axum::http::{HeaderValue, Method, header};
use axum::routing::{get, post};
use tower_http::cors::{AllowOrigin, CorsLayer};
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
    /// Deployment options for the CDS Hooks routes.
    pub cds: CdsSettings,
}

/// Deployment options for the CDS Hooks routes. The defaults suit a real
/// EHR install: no CORS and no per-study entries in discovery.
#[derive(Debug, Clone, Default)]
pub struct CdsSettings {
    /// Browser origins allowed to call `/cds-services` (from `CDS_CORS_ORIGINS`).
    pub cors_origins: CorsOrigins,
    /// Whether discovery lists one service per open study (from
    /// `CDS_DISCOVERY_LIST_STUDIES`). The per-study endpoint works either way.
    pub list_studies_in_discovery: bool,
}

/// Which browser origins may call the CDS Hooks routes.
///
/// EHRs such as Epic call server-to-server and never need CORS; only
/// browser-based clients (e.g. the public CDS Hooks sandbox) do.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum CorsOrigins {
    /// No CORS headers at all.
    #[default]
    None,
    /// Any origin (`*`). Never appropriate for a server holding real data.
    Any,
    /// Exactly these origins.
    List(Vec<HeaderValue>),
}

impl CorsOrigins {
    /// Parse a comma-separated origin list. Unset or blank means
    /// [`CorsOrigins::None`]; a lone `*` means [`CorsOrigins::Any`].
    /// Entries that are not valid header values are skipped with a warning.
    pub fn parse(raw: Option<&str>) -> Self {
        let entries: Vec<&str> = raw
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        match entries.as_slice() {
            [] => CorsOrigins::None,
            ["*"] => CorsOrigins::Any,
            _ => CorsOrigins::List(
                entries
                    .into_iter()
                    .filter_map(|origin| {
                        let value = HeaderValue::from_str(origin.trim_end_matches('/')).ok();
                        if value.is_none() {
                            tracing::warn!(origin, "ignoring invalid CDS_CORS_ORIGINS entry");
                        }
                        value
                    })
                    .collect(),
            ),
        }
    }

    /// The CORS layer for these origins, or `None` when CORS is disabled.
    fn layer(&self) -> Option<CorsLayer> {
        let allow_origin = match self {
            CorsOrigins::None => return None,
            CorsOrigins::Any => AllowOrigin::any(),
            CorsOrigins::List(origins) => AllowOrigin::list(origins.iter().cloned()),
        };
        // No cookies are used on these routes, so credentials stay disallowed.
        // Private Network Access lets a public page (the sandbox) reach a
        // server on localhost or an internal address.
        Some(
            CorsLayer::new()
                .allow_origin(allow_origin)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
                .allow_private_network(true),
        )
    }
}

/// Build the full application router with shared state.
pub fn app(state: Arc<AppState>) -> Router {
    let cds_cors = state.cds.cors_origins.layer();

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

    let mut cds = Router::new()
        .route("/cds-services", get(cds_hooks::discover_services))
        .route("/cds-services/randomization", post(cds_hooks::randomize))
        .route("/cds-services/{study_id}", post(cds_hooks::randomize_study));
    if let Some(cors) = cds_cors {
        cds = cds.layer(cors);
    }

    Router::new()
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready))
        .route("/api/me", get(me::get_me))
        .nest("/api/v1", api_v1)
        .merge(cds)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_origins_unset_or_blank_is_none() {
        assert_eq!(CorsOrigins::parse(None), CorsOrigins::None);
        assert_eq!(CorsOrigins::parse(Some(" , ")), CorsOrigins::None);
    }

    #[test]
    fn cors_origins_star_is_any() {
        assert_eq!(CorsOrigins::parse(Some(" * ")), CorsOrigins::Any);
    }

    #[test]
    fn cors_origins_list_is_trimmed() {
        assert_eq!(
            CorsOrigins::parse(Some("https://sandbox.cds-hooks.org/, https://demo.example")),
            CorsOrigins::List(vec![
                HeaderValue::from_static("https://sandbox.cds-hooks.org"),
                HeaderValue::from_static("https://demo.example"),
            ])
        );
    }
}
