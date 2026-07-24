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
    pub tls: TlsStatus,
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

/// TLS certificate status, distinguishing "no cert configured" from a genuine
/// deployment error (unreadable/unparseable file) from a successfully-read
/// certificate chain. Serialized as a tagged union on `state`.
#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum TlsStatus {
    /// No `CERT_PATH` was configured. Expected when TLS is terminated upstream.
    NotConfigured,
    /// A cert path was configured but could not be read or parsed. This is a
    /// deployment problem worth surfacing, not a benign "off" state.
    Error { message: String },
    /// The certificate chain was read successfully.
    Ok(TlsInfo),
}

/// Details of a successfully-parsed certificate chain. Expiry fields reflect
/// the **soonest-expiring** certificate in the chain (an expiring intermediate
/// breaks the connection just as an expiring leaf does).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsInfo {
    /// Subject of the leaf (end-entity) certificate.
    pub subject: String,
    /// Issuer of the leaf certificate.
    pub issuer: String,
    /// `not_before` of the soonest-expiring certificate in the chain.
    pub not_before: DateTime<Utc>,
    /// `not_after` of the soonest-expiring certificate in the chain.
    pub not_after: DateTime<Utc>,
    /// Whole days until the soonest expiry (truncated; may be negative).
    pub days_remaining: i64,
    /// True only if every certificate in the chain is currently within its
    /// validity window.
    pub valid: bool,
    /// Number of certificates parsed from the PEM file.
    pub chain_length: usize,
}

/// `GET /api/v1/system/status` — comprehensive system health check.
pub async fn get_system_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let now = Utc::now();

    let postgres = probe(state.pg_health.as_ref()).await;
    let redis = probe(state.redis_health.as_ref()).await;

    // TLS certificate check
    let tls = read_tls_status(&state.cert_path);

    // Overall status. A configured-but-broken cert (Error) or one within its
    // final week degrades health; a NotConfigured cert is treated as fine
    // because TLS may be terminated upstream.
    let all_up = postgres.status == ServiceStatus::Up && redis.status == ServiceStatus::Up;
    let tls_valid = match &tls {
        TlsStatus::NotConfigured => true,
        TlsStatus::Error { .. } => false,
        TlsStatus::Ok(info) => info.valid && info.days_remaining > 7,
    };
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

/// Read and parse the TLS certificate chain at `cert_path`.
///
/// Reports the soonest-expiring certificate across the whole chain so an
/// expiring intermediate is not masked by a longer-lived leaf. Distinguishes
/// "not configured" from genuine read/parse errors so the dashboard can flag
/// deployment problems rather than silently showing nothing.
fn read_tls_status(cert_path: &Option<String>) -> TlsStatus {
    let Some(path) = cert_path.as_ref() else {
        return TlsStatus::NotConfigured;
    };

    let contents = match std::fs::read(path) {
        Ok(c) => c,
        Err(e) => {
            warn!(path, error = %e, "failed to read TLS certificate");
            return TlsStatus::Error {
                message: format!("cannot read certificate file: {e}"),
            };
        }
    };

    let blocks = match pem::parse_many(&contents) {
        Ok(blocks) => blocks,
        Err(e) => {
            warn!(error = %e, "failed to parse PEM");
            return TlsStatus::Error {
                message: format!("invalid PEM: {e}"),
            };
        }
    };

    // Consider only CERTIFICATE blocks; a fullchain PEM may also carry keys.
    let cert_blocks: Vec<_> = blocks
        .iter()
        .filter(|b| b.tag() == "CERTIFICATE")
        .collect();

    if cert_blocks.is_empty() {
        warn!(path, "PEM contained no CERTIFICATE blocks");
        return TlsStatus::Error {
            message: "no certificates found in PEM file".to_owned(),
        };
    }

    let mut leaf: Option<(String, String)> = None;
    let mut validities = Vec::with_capacity(cert_blocks.len());

    for (index, block) in cert_blocks.iter().enumerate() {
        let cert = match x509_parser::parse_x509_certificate(block.contents()) {
            Ok((_, cert)) => cert,
            Err(e) => {
                warn!(index, error = %e, "failed to parse X.509 certificate");
                return TlsStatus::Error {
                    message: format!("invalid X.509 certificate at position {index}: {e}"),
                };
            }
        };

        let (Some(not_before), Some(not_after)) = (
            DateTime::from_timestamp(cert.validity().not_before.to_datetime().unix_timestamp(), 0),
            DateTime::from_timestamp(cert.validity().not_after.to_datetime().unix_timestamp(), 0),
        ) else {
            return TlsStatus::Error {
                message: format!("certificate at position {index} has an out-of-range validity date"),
            };
        };

        // The first block in a chain PEM is the leaf (end-entity) certificate.
        if index == 0 {
            leaf = Some((cert.subject().to_string(), cert.issuer().to_string()));
        }

        validities.push(CertValidity {
            not_before,
            not_after,
        });
    }

    let (subject, issuer) = leaf.expect("cert_blocks is non-empty so leaf is set");
    TlsStatus::Ok(summarize_chain(subject, issuer, &validities, Utc::now()))
}

/// The validity window of a single certificate in a chain.
#[derive(Debug, Clone, Copy)]
struct CertValidity {
    not_before: DateTime<Utc>,
    not_after: DateTime<Utc>,
}

/// Summarize a certificate chain into a [`TlsInfo`], reporting the
/// soonest-expiring certificate and treating the chain as valid only if every
/// certificate is currently within its validity window.
///
/// `validities` must be non-empty and ordered leaf-first.
fn summarize_chain(
    subject: String,
    issuer: String,
    validities: &[CertValidity],
    now: DateTime<Utc>,
) -> TlsInfo {
    let soonest = validities
        .iter()
        .min_by_key(|v| v.not_after)
        .expect("summarize_chain requires a non-empty chain");
    let all_valid = validities
        .iter()
        .all(|v| now >= v.not_before && now <= v.not_after);

    TlsInfo {
        subject,
        issuer,
        not_before: soonest.not_before,
        not_after: soonest.not_after,
        days_remaining: (soonest.not_after - now).num_days(),
        valid: all_valid,
        chain_length: validities.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconfigured_path_is_not_configured() {
        assert!(matches!(read_tls_status(&None), TlsStatus::NotConfigured));
    }

    #[test]
    fn missing_file_is_an_error() {
        let status = read_tls_status(&Some("/nonexistent/cert.pem".to_owned()));
        assert!(matches!(status, TlsStatus::Error { .. }));
    }

    #[test]
    fn garbage_contents_is_an_error() {
        let dir = std::env::temp_dir();
        let path = dir.join("pocr-test-garbage.pem");
        std::fs::write(&path, b"not a pem file").unwrap();
        let status = read_tls_status(&Some(path.to_string_lossy().into_owned()));
        let _ = std::fs::remove_file(&path);
        assert!(matches!(status, TlsStatus::Error { .. }));
    }

    fn validity(not_before_days: i64, not_after_days: i64, now: DateTime<Utc>) -> CertValidity {
        CertValidity {
            not_before: now + chrono::Duration::days(not_before_days),
            not_after: now + chrono::Duration::days(not_after_days),
        }
    }

    #[test]
    fn chain_reports_soonest_expiry_not_the_leaf() {
        let now = Utc::now();
        // Leaf lives 90 days; the intermediate expires in 10. The soonest wins.
        let chain = [validity(-1, 90, now), validity(-30, 10, now)];
        let info = summarize_chain("CN=leaf".into(), "CN=ca".into(), &chain, now);

        assert_eq!(info.chain_length, 2);
        assert!(info.valid);
        assert_eq!(info.days_remaining, 10);
        assert_eq!(info.subject, "CN=leaf");
    }

    #[test]
    fn chain_is_invalid_if_any_cert_is_expired() {
        let now = Utc::now();
        // Leaf is current, but the intermediate already expired yesterday.
        let chain = [validity(-1, 90, now), validity(-60, -1, now)];
        let info = summarize_chain("CN=leaf".into(), "CN=ca".into(), &chain, now);

        assert!(!info.valid);
        assert!(info.days_remaining < 0);
    }

    #[test]
    fn single_cert_chain_reports_its_own_expiry() {
        let now = Utc::now();
        let chain = [validity(-1, 45, now)];
        let info = summarize_chain("CN=solo".into(), "CN=solo".into(), &chain, now);

        assert_eq!(info.chain_length, 1);
        assert!(info.valid);
        assert_eq!(info.days_remaining, 45);
    }
}
