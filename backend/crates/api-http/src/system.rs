//! System status endpoint — reports infrastructure health and TLS certificate validity.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use tracing::warn;

use pocr_core::ports::HealthCheckable;

use crate::AppState;

/// Top-level system status response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub status: OverallStatus,
    pub uptime_seconds: u64,
    pub server_time: DateTime<Utc>,
    pub services: ServiceStatuses,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsStatus>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OverallStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatuses {
    pub postgres: ServiceHealth,
    pub redis: ServiceHealth,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHealth {
    pub status: ServiceStatus,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Up,
    Down,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsStatus {
    pub subject: String,
    pub issuer: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub days_remaining: i64,
    pub valid: bool,
}

/// `GET /api/v1/system/status` — comprehensive system health check.
pub async fn get_system_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let now = Utc::now();

    let postgres = probe(state.pg_health.as_ref()).await;
    let redis = probe(state.redis_health.as_ref()).await;

    // TLS certificate check
    let tls = read_tls_status(&state.cert_path);

    // Overall status
    let all_up = postgres.status == ServiceStatus::Up && redis.status == ServiceStatus::Up;
    let tls_valid = tls.as_ref().is_none_or(|t| t.valid && t.days_remaining > 7);
    let status = if all_up && tls_valid {
        OverallStatus::Healthy
    } else if all_up || tls_valid {
        OverallStatus::Degraded
    } else {
        OverallStatus::Unhealthy
    };

    let uptime_seconds = state.started_at.elapsed().as_secs();

    Json(SystemStatus {
        status,
        uptime_seconds,
        server_time: now,
        services: ServiceStatuses { postgres, redis },
        tls,
    })
}

/// Ping a service and report its status alongside the measured latency.
async fn probe(service: &dyn HealthCheckable) -> ServiceHealth {
    let start = Instant::now();
    let result = service.ping().await;
    let latency_ms = start.elapsed().as_millis() as u64;
    match result {
        Ok(()) => ServiceHealth {
            status: ServiceStatus::Up,
            latency_ms,
            error: None,
        },
        Err(e) => ServiceHealth {
            status: ServiceStatus::Down,
            latency_ms,
            error: Some(e),
        },
    }
}

fn read_tls_status(cert_path: &Option<String>) -> Option<TlsStatus> {
    let path = cert_path.as_ref()?;
    let contents = match std::fs::read(path) {
        Ok(c) => c,
        Err(e) => {
            warn!(path, error = %e, "failed to read TLS certificate");
            return None;
        }
    };

    let pem_data = match pem::parse(&contents) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "failed to parse PEM");
            return None;
        }
    };

    let (_, cert) = match x509_parser::parse_x509_certificate(pem_data.contents()) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to parse X.509 certificate");
            return None;
        }
    };

    let not_before = DateTime::from_timestamp(
        cert.validity().not_before.to_datetime().unix_timestamp(),
        0,
    )?;
    let not_after = DateTime::from_timestamp(
        cert.validity().not_after.to_datetime().unix_timestamp(),
        0,
    )?;
    let now = Utc::now();
    let days_remaining = (not_after - now).num_days();
    let valid = now >= not_before && now <= not_after;

    Some(TlsStatus {
        subject: cert.subject().to_string(),
        issuer: cert.issuer().to_string(),
        not_before,
        not_after,
        days_remaining,
        valid,
    })
}
