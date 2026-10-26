use tracing::{Level, info};
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone, Copy, Debug)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Human-identifiable name for this process (api/worker/migrator/etc).
    pub service_name: &'static str,

    /// If `RUST_LOG` is not set, this is used.
    pub default_filter: &'static str,

    /// Preferred log format (pretty vs json).
    pub format: LogFormat,

    /// Emit span lifecycle events (useful for debugging, noisy in prod).
    pub span_events: bool,

    /// Use a non-blocking writer (recommended for servers/workers).
    pub non_blocking: bool,
}

impl Config {
    pub fn for_service(service_name: &'static str) -> Self {
        Self {
            service_name,
            default_filter: "info",
            format: LogFormat::Pretty,
            span_events: false,
            non_blocking: true,
        }
    }

    /// Environment overrides:
    /// - LOG_JSON: true/false-ish values override cfg.format
    /// - LOG_SPAN_EVENTS: true/false-ish values override cfg.span_events
    ///
    /// Unset => keep defaults.
    pub fn from_env(service_name: &'static str) -> Self {
        let mut cfg = Self::for_service(service_name);

        if let Some(v) = env_bool("LOG_JSON") {
            cfg.format = if v {
                LogFormat::Json
            } else {
                LogFormat::Pretty
            };
        }

        if let Some(v) = env_bool("LOG_SPAN_EVENTS") {
            cfg.span_events = v;
        }

        cfg
    }
}

/// Hold guards/resources that must live for the entire process lifetime.
pub struct Guard {
    _non_blocking: Option<tracing_appender::non_blocking::WorkerGuard>,
    _root: tracing::span::EnteredSpan,
}

pub fn init(cfg: Config) -> Guard {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(cfg.default_filter));

    let err_layer = tracing_error::ErrorLayer::default();

    // Optional non-blocking stdout to reduce contention/latency under load.
    let (writer, guard) = if cfg.non_blocking {
        let (nb, g) = tracing_appender::non_blocking(std::io::stdout());
        (Writer::NonBlocking(nb), Some(g))
    } else {
        (Writer::Blocking, None)
    };

    let fmt_layer = match writer {
        Writer::NonBlocking(w) => {
            let base = tracing_subscriber::fmt::layer()
                .with_writer(w)
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true);

            let base = if cfg.span_events {
                base.with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            } else {
                base
            };

            match cfg.format {
                LogFormat::Pretty => base.pretty().boxed(),
                LogFormat::Json => base.json().boxed(),
            }
        }
        Writer::Blocking => {
            let base = tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true);

            let base = if cfg.span_events {
                base.with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            } else {
                base
            };

            match cfg.format {
                LogFormat::Pretty => base.pretty().boxed(),

                // Include span context in JSON for better observability.
                LogFormat::Json => base
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .boxed(),
            }
        }
    };

    tracing_subscriber::registry()
        .with(database_log_filter())
        .with(env_filter)
        .with(err_layer)
        .with(fmt_layer)
        .init();

    let root = tracing::span!(Level::INFO, "process", service.name = cfg.service_name).entered();

    info!("observability initialized");

    Guard {
        _non_blocking: guard,
        _root: root,
    }
}

enum Writer {
    Blocking,
    NonBlocking(tracing_appender::non_blocking::NonBlocking),
}

/// Returns:
/// - Some(true) / Some(false) if the var is set and parses
/// - None if the var is unset or invalid
///
/// Accepted true:  1, true, t, yes, y, on  (case-insensitive)
/// Accepted false: 0, false, f, no, n, off (case-insensitive)
///
/// Panics if the var is set but has an invalid value.
fn env_bool(key: &str) -> Option<bool> {
    let raw = std::env::var_os(key)?;
    let s = raw.to_string_lossy().trim().to_ascii_lowercase();

    let v = match s.as_str() {
        "1" | "true" | "t" | "yes" | "y" | "on" => true,
        "0" | "false" | "f" | "no" | "n" | "off" => false,
        _ => {
            panic!("{key} has invalid value {s:?}; use true/false (or 1/0)");
        }
    };

    Some(v)
}

fn database_log_filter<S: tracing::Subscriber>() -> impl Layer<S> {
    // Tiberius emits raw server messages at ERROR/INFO and row data at TRACE.
    // Application errors preserve safe categories and server codes instead.
    tracing_subscriber::filter::filter_fn(|metadata| {
        metadata.target() != "tiberius" && !metadata.target().starts_with("tiberius::")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tracing::Subscriber;
    use tracing_subscriber::layer::Context;

    struct EventCount(Arc<AtomicUsize>);

    impl<S: Subscriber> Layer<S> for EventCount {
        fn on_event(&self, _: &tracing::Event<'_>, _: Context<'_, S>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn database_driver_events_are_suppressed_even_at_trace_level() {
        let count = Arc::new(AtomicUsize::new(0));
        let subscriber = tracing_subscriber::registry()
            .with(database_log_filter())
            .with(EventCount(count.clone()));
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!(target: "tiberius::tds::stream::token", "sensitive server error");
            tracing::trace!(target: "tiberius::tds::stream::token", "sensitive row data");
            tracing::info!(target: "tiberius", "sensitive server message");
            assert_eq!(count.load(Ordering::Relaxed), 0);
            tracing::error!(target: "infra_caboodle", "sanitized application error");
            assert_eq!(count.load(Ordering::Relaxed), 1);
        });
    }
}
