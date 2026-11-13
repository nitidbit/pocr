//! Deployment environment for a randomization request.
//!
//! Epic operates multiple instances (production, test/training). A request
//! from a non-production instance must never touch production randomization
//! state: it draws from an independent block sequence, deduplicates
//! independently, and writes to a separate partition of the records table.
//!
//! The environment for a request is resolved from the CDS Hooks `fhirServer`
//! base URL via an operator-supplied prefix map. Resolution **fails closed**:
//! a missing or unrecognized URL is rejected rather than defaulted, so an
//! unknown instance can never contaminate production data.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The deployment environment a randomization request originates from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Environment {
    /// Live clinical use. Randomizations count toward the real trial.
    Production,
    /// Test/training use. Fully isolated from production state and purgeable.
    NonProduction,
}

impl Environment {
    /// The lowercase string form used in cache keys and stored records.
    pub fn as_str(self) -> &'static str {
        match self {
            Environment::Production => "production",
            Environment::NonProduction => "nonproduction",
        }
    }

    /// Parses the lowercase string form (inverse of [`Environment::as_str`]).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "production" => Some(Environment::Production),
            "nonproduction" => Some(Environment::NonProduction),
            _ => None,
        }
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error resolving a request's environment from its `fhirServer` URL.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum EnvironmentError {
    /// The request carried no `fhirServer` URL.
    #[error("request has no fhirServer URL")]
    MissingUrl,
    /// The URL matched no configured prefix.
    #[error("no environment configured for fhirServer URL: {0}")]
    UnknownUrl(String),
}

/// An operator-supplied map from `fhirServer` URL prefixes to environments.
///
/// Built once at startup (e.g. from the `FHIR_ENV_MAP` env var) and consulted
/// per request. Matching is longest-prefix-wins so that a more specific URL
/// (e.g. a production sub-path) takes precedence over a broader test prefix.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentMap {
    /// Sorted longest-prefix-first for deterministic resolution.
    entries: Vec<(String, Environment)>,
}

impl EnvironmentMap {
    /// Builds a map from `(url_prefix, environment)` pairs.
    pub fn new(entries: impl IntoIterator<Item = (String, Environment)>) -> Self {
        let mut entries: Vec<(String, Environment)> = entries.into_iter().collect();
        // Longest prefix first so resolution picks the most specific match.
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.0.len()));
        Self { entries }
    }

    /// Resolves an optional `fhirServer` URL to an environment.
    ///
    /// Fails closed and never defaults.
    ///
    /// # Errors
    ///
    /// Returns [`EnvironmentError::MissingUrl`] for `None` and
    /// [`EnvironmentError::UnknownUrl`] for a URL matching no prefix.
    pub fn resolve(&self, fhir_server: Option<&str>) -> Result<Environment, EnvironmentError> {
        let url = fhir_server.ok_or(EnvironmentError::MissingUrl)?;
        self.entries
            .iter()
            .find(|(prefix, _)| url.starts_with(prefix.as_str()))
            .map(|(_, env)| *env)
            .ok_or_else(|| EnvironmentError::UnknownUrl(url.to_string()))
    }

    /// Whether the map has no configured prefixes.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
