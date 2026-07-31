//! Process-level observability setup for Blackflower executables.
//!
//! Runtime libraries emit `tracing`, `metrics`, and `profiling` signals but do
//! not choose global subscribers, recorders, or profiler backends. Executables
//! call [`init`] once and retain the returned [`ObservabilityGuard`] until
//! shutdown.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroUsize;

use metrics_exporter_prometheus::PrometheusBuilder;
use tracing_appender::non_blocking::{ErrorCounter, NonBlocking, NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::{
    EnvFilter, Layer, Registry, layer::SubscriberExt as _, util::SubscriberInitExt as _,
    util::TryInitError,
};

#[cfg(feature = "profile-with-tracy")]
use tracing_subscriber::filter::filter_fn;

const DEFAULT_LOG_BUFFER_LINES: usize = 8_192;
const DEFAULT_SERVER_METRICS_PORT: u16 = 9_000;

/// Format used for process logs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable compact logs for interactive development.
    #[default]
    Compact,
    /// Multi-line human-readable logs for interactive diagnosis.
    Pretty,
    /// Structured newline-delimited JSON for production ingestion.
    Json,
}

impl LogFormat {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Pretty => "pretty",
            Self::Json => "json",
        }
    }
}

/// Process-level observability configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityConfig {
    service_name: &'static str,
    service_version: &'static str,
    log_format: LogFormat,
    default_log_filter: &'static str,
    log_buffer_lines: usize,
    metrics_bind_address: Option<SocketAddr>,
}

impl ObservabilityConfig {
    /// Construct development-oriented defaults without a metrics endpoint.
    #[must_use]
    pub fn client(service_name: &'static str, service_version: &'static str) -> Self {
        Self {
            service_name,
            service_version,
            log_format: LogFormat::Compact,
            default_log_filter: "info",
            log_buffer_lines: DEFAULT_LOG_BUFFER_LINES,
            metrics_bind_address: None,
        }
    }

    /// Construct server defaults with compact logs and a loopback Prometheus endpoint.
    #[must_use]
    pub fn server(service_name: &'static str, service_version: &'static str) -> Self {
        Self {
            metrics_bind_address: Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                DEFAULT_SERVER_METRICS_PORT,
            )),
            ..Self::client(service_name, service_version)
        }
    }

    /// Override the log format.
    #[must_use]
    pub const fn with_log_format(mut self, log_format: LogFormat) -> Self {
        self.log_format = log_format;
        self
    }

    /// Override the fallback filter used when `RUST_LOG` is absent or invalid.
    #[must_use]
    pub const fn with_default_log_filter(mut self, filter: &'static str) -> Self {
        self.default_log_filter = filter;
        self
    }

    /// Override the bounded non-blocking log queue capacity.
    #[must_use]
    pub const fn with_log_buffer_lines(mut self, lines: NonZeroUsize) -> Self {
        self.log_buffer_lines = lines.get();
        self
    }

    /// Enable or disable the Prometheus scrape listener.
    #[must_use]
    pub const fn with_metrics_bind_address(mut self, address: Option<SocketAddr>) -> Self {
        self.metrics_bind_address = address;
        self
    }

    /// Return the configured service name.
    #[must_use]
    pub const fn service_name(&self) -> &'static str {
        self.service_name
    }

    /// Return the configured metrics listener address.
    #[must_use]
    pub const fn metrics_bind_address(&self) -> Option<SocketAddr> {
        self.metrics_bind_address
    }
}

/// Failure while installing process-level observability.
#[derive(Debug, thiserror::Error)]
pub enum InitError {
    /// The process-wide tracing subscriber could not be installed.
    #[error("failed to install tracing subscriber")]
    Tracing(#[source] TryInitError),
}

/// Lifetime guard for the non-blocking log writer.
///
/// Retain this value until process shutdown so queued records are flushed.
pub struct ObservabilityGuard {
    service_name: &'static str,
    service_version: &'static str,
    prometheus_listener_active: bool,
    dropped_log_lines: ErrorCounter,
    _writer_guard: WorkerGuard,
}

impl ObservabilityGuard {
    /// Return the number of log records dropped because the queue was full.
    #[must_use]
    pub fn dropped_log_lines(&self) -> usize {
        self.dropped_log_lines.dropped_lines()
    }

    /// Report whether this process installed its Prometheus listener.
    #[must_use]
    pub const fn prometheus_listener_active(&self) -> bool {
        self.prometheus_listener_active
    }

    /// Publish the current non-blocking writer health to the metrics recorder.
    pub fn report_health(&self) {
        let dropped = u64::try_from(self.dropped_log_lines()).unwrap_or(u64::MAX);
        metrics::counter!("blackflower_observability_log_lines_dropped_total").absolute(dropped);
    }
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        self.report_health();
        tracing::info!(
            target: "blackflower_observability",
            event_name = "service_stopping",
            service = self.service_name,
            version = self.service_version,
            prometheus_listener_active = self.prometheus_listener_active,
            dropped_log_lines = self.dropped_log_lines(),
            "observability stopping",
        );
    }
}

/// Install the metrics recorder, non-blocking logger, and tracing subscriber.
///
/// The `profiling` crate remains backend-neutral. Enabling this crate's
/// `profile-with-tracy` feature maps `profiling` scopes to tracing spans and
/// installs a `tracing-tracy` layer.
pub fn init(config: &ObservabilityConfig) -> Result<ObservabilityGuard, InitError> {
    let (writer, writer_guard) = NonBlockingBuilder::default()
        .buffered_lines_limit(config.log_buffer_lines)
        .lossy(true)
        .thread_name("blackflower-log-writer")
        .finish(std::io::stderr());
    let dropped_log_lines = writer.error_counter();
    install_tracing(config, writer)?;

    let prometheus_listener_active = match install_metrics(config.metrics_bind_address) {
        Ok(active) => active,
        Err(error) => {
            tracing::error!(
                target: "blackflower_observability",
                event_name = "metrics_exporter_unavailable",
                error = %error,
                metrics_bind_address = ?config.metrics_bind_address,
                "metrics unavailable",
            );
            false
        }
    };
    describe_metrics();

    tracing::info!(
        target: "blackflower_observability",
        event_name = "service_started",
        service = config.service_name,
        version = config.service_version,
        log_format = config.log_format.as_str(),
        metrics_bind_address = ?config.metrics_bind_address,
        prometheus_listener_active,
        "observability initialized",
    );

    Ok(ObservabilityGuard {
        service_name: config.service_name,
        service_version: config.service_version,
        prometheus_listener_active,
        dropped_log_lines,
        _writer_guard: writer_guard,
    })
}

fn install_metrics(
    address: Option<SocketAddr>,
) -> Result<bool, metrics_exporter_prometheus::BuildError> {
    if let Some(address) = address {
        PrometheusBuilder::new()
            .with_http_listener(address)
            .install()?;
        return Ok(true);
    }
    Ok(false)
}

fn install_tracing(config: &ObservabilityConfig, writer: NonBlocking) -> Result<(), InitError> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_error| EnvFilter::new(config.default_log_filter));
    let fmt_layer: Box<dyn Layer<Registry> + Send + Sync> = match config.log_format {
        LogFormat::Compact => tracing_subscriber::fmt::layer()
            .compact()
            .with_ansi(true)
            .with_writer(writer)
            .boxed(),
        LogFormat::Pretty => tracing_subscriber::fmt::layer()
            .pretty()
            .with_ansi(true)
            .with_writer(writer)
            .boxed(),
        LogFormat::Json => tracing_subscriber::fmt::layer()
            .json()
            .flatten_event(true)
            .with_ansi(false)
            .with_current_span(true)
            .with_span_list(true)
            .with_writer(writer)
            .boxed(),
    };
    let subscriber = tracing_subscriber::registry().with(fmt_layer.with_filter(filter));
    #[cfg(feature = "profile-with-tracy")]
    let subscriber = subscriber
        .with(tracing_tracy::TracyLayer::default().with_filter(filter_fn(is_tracy_signal)));
    subscriber.try_init().map_err(InitError::Tracing)
}

#[cfg(feature = "profile-with-tracy")]
fn is_tracy_signal(metadata: &tracing::Metadata<'_>) -> bool {
    metadata.is_span() || metadata.fields().field("tracy.frame_mark").is_some()
}

fn describe_metrics() {
    metrics::describe_counter!(
        "blackflower_observability_log_lines_dropped_total",
        metrics::Unit::Count,
        "Log records dropped by the bounded non-blocking writer",
    );
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
