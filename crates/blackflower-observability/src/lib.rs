//! Process-level observability setup for Blackflower executables.
//!
//! Runtime libraries emit `tracing`, `metrics`, and `profiling` signals but do
//! not choose global subscribers, recorders, or profiler backends. Executables
//! call [`init`] once and retain the returned [`ObservabilityGuard`] until
//! shutdown.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroUsize;
use std::time::Duration;

use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_util::MetricKindMask;
use strum::IntoStaticStr;
use tracing_appender::non_blocking::{ErrorCounter, NonBlocking, NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::{
    EnvFilter, Layer, Registry, layer::SubscriberExt as _, util::SubscriberInitExt as _,
    util::TryInitError,
};

mod foreground_logs;
mod host_metrics;

pub use foreground_logs::{ForegroundLogControl, ForegroundLogEvent, ForegroundLogLevel};
use host_metrics::HostMetricsCollector;

#[cfg(feature = "profile-with-tracy")]
use tracing_subscriber::filter::filter_fn;

const DEFAULT_LOG_BUFFER_LINES: usize = 8_192;
const DEFAULT_FOREGROUND_LOG_CAPACITY: usize = 4_096;
const DEFAULT_SERVER_METRICS_PORT: u16 = 9_000;
const METRICS_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Format used for process logs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable compact logs for interactive development.
    #[default]
    Compact,
    /// Multi-line human-readable logs for interactive diagnosis.
    Pretty,
}

impl LogFormat {
    fn as_str(self) -> &'static str {
        self.into()
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
    host_metrics_enabled: bool,
    foreground_log_capacity: Option<usize>,
    foreground_log_level: ForegroundLogLevel,
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
            host_metrics_enabled: false,
            foreground_log_capacity: None,
            foreground_log_level: ForegroundLogLevel::Info,
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
            host_metrics_enabled: true,
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

    /// Enable or disable collection of host and current-process metrics.
    #[must_use]
    pub const fn with_host_metrics(mut self, enabled: bool) -> Self {
        self.host_metrics_enabled = enabled;
        self
    }

    /// Capture structured logs for an interactive foreground consumer.
    ///
    /// Enabling this output suppresses formatted terminal logs so they cannot
    /// corrupt the terminal UI. The capture queue is bounded and lossy.
    #[must_use]
    pub const fn with_foreground_logs(
        mut self,
        level: ForegroundLogLevel,
        capacity: NonZeroUsize,
    ) -> Self {
        self.foreground_log_capacity = Some(capacity.get());
        self.foreground_log_level = level;
        self
    }

    /// Capture informational structured logs using the shared foreground
    /// dashboard queue policy.
    #[must_use]
    pub const fn with_default_foreground_logs(mut self) -> Self {
        self.foreground_log_capacity = Some(DEFAULT_FOREGROUND_LOG_CAPACITY);
        self.foreground_log_level = ForegroundLogLevel::Info;
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

    /// Return whether embedded host metrics collection is enabled.
    #[must_use]
    pub const fn host_metrics_enabled(&self) -> bool {
        self.host_metrics_enabled
    }

    /// Return whether structured foreground log capture is enabled.
    #[must_use]
    pub const fn foreground_logs_enabled(&self) -> bool {
        self.foreground_log_capacity.is_some()
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
    host_metrics_active: bool,
    foreground_log_receiver: Option<std::sync::mpsc::Receiver<ForegroundLogEvent>>,
    foreground_log_control: Option<ForegroundLogControl>,
    _host_metrics_collector: Option<HostMetricsCollector>,
    formatted_log_output: Option<FormattedLogOutput>,
}

impl ObservabilityGuard {
    /// Return the number of log records dropped because the queue was full.
    #[must_use]
    pub fn dropped_log_lines(&self) -> usize {
        self.formatted_log_output
            .as_ref()
            .map_or(0, |output| output.dropped_log_lines.dropped_lines())
    }

    /// Report whether formatted process logs can write to the terminal.
    #[must_use]
    pub const fn formatted_log_output_active(&self) -> bool {
        self.formatted_log_output.is_some()
    }

    /// Report whether this process installed its Prometheus listener.
    #[must_use]
    pub const fn prometheus_listener_active(&self) -> bool {
        self.prometheus_listener_active
    }

    /// Report whether this process started its embedded host collector.
    #[must_use]
    pub const fn host_metrics_active(&self) -> bool {
        self.host_metrics_active
    }

    /// Take the single structured foreground log receiver and its control.
    ///
    /// Only one foreground consumer can own the receiver. The control remains
    /// cheap to clone so capture level and dropped-record health stay visible.
    pub fn take_foreground_logs(
        &mut self,
    ) -> Option<(
        std::sync::mpsc::Receiver<ForegroundLogEvent>,
        ForegroundLogControl,
    )> {
        let receiver = self.foreground_log_receiver.take()?;
        let control = self.foreground_log_control.as_ref()?.clone();
        Some((receiver, control))
    }

    /// Publish the current non-blocking writer health to the metrics recorder.
    pub fn report_log_pipeline_health(&self) {
        let dropped = u64::try_from(self.dropped_log_lines()).unwrap_or(u64::MAX);
        metrics::counter!("blackflower_observability_log_lines_dropped_total").absolute(dropped);
        if let Some(control) = &self.foreground_log_control {
            metrics::counter!("blackflower_observability_foreground_log_lines_dropped_total")
                .absolute(control.dropped_events());
        }
    }
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        self.report_log_pipeline_health();
        tracing::info!(
            target: "blackflower_observability",
            event_name = "service_stopping",
            service = self.service_name,
            version = self.service_version,
            prometheus_listener_active = self.prometheus_listener_active,
            host_metrics_active = self.host_metrics_active,
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
    let formatted_log_output = create_formatted_log_output(config);
    let formatted_writer = formatted_log_output
        .as_ref()
        .map(|output| output.writer.clone());
    let foreground_logs = install_tracing(config, formatted_writer)?;

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
    let host_metrics_collector = (config.host_metrics_enabled && prometheus_listener_active)
        .then(HostMetricsCollector::start)
        .flatten();
    let host_metrics_active = host_metrics_collector.is_some();

    tracing::info!(
        target: "blackflower_observability",
        event_name = "service_started",
        service = config.service_name,
        version = config.service_version,
        log_format = config.log_format.as_str(),
        metrics_bind_address = ?config.metrics_bind_address,
        prometheus_listener_active,
        host_metrics_active,
        "observability initialized",
    );

    let (foreground_log_receiver, foreground_log_control) = split_foreground_logs(foreground_logs);

    Ok(ObservabilityGuard {
        service_name: config.service_name,
        service_version: config.service_version,
        prometheus_listener_active,
        host_metrics_active,
        foreground_log_receiver,
        foreground_log_control,
        _host_metrics_collector: host_metrics_collector,
        formatted_log_output,
    })
}

struct FormattedLogOutput {
    writer: NonBlocking,
    dropped_log_lines: ErrorCounter,
    _writer_guard: WorkerGuard,
}

fn create_formatted_log_output(config: &ObservabilityConfig) -> Option<FormattedLogOutput> {
    if config.foreground_logs_enabled() {
        return None;
    }
    let (writer, writer_guard) = NonBlockingBuilder::default()
        .buffered_lines_limit(config.log_buffer_lines)
        .lossy(true)
        .thread_name("blackflower-log-writer")
        .finish(std::io::stderr());
    let dropped_log_lines = writer.error_counter();
    Some(FormattedLogOutput {
        writer,
        dropped_log_lines,
        _writer_guard: writer_guard,
    })
}

fn split_foreground_logs(
    logs: Option<foreground_logs::ForegroundLogs>,
) -> (
    Option<std::sync::mpsc::Receiver<ForegroundLogEvent>>,
    Option<ForegroundLogControl>,
) {
    logs.map_or((None, None), |logs| {
        (Some(logs.receiver), Some(logs.control))
    })
}

fn install_metrics(
    address: Option<SocketAddr>,
) -> Result<bool, metrics_exporter_prometheus::BuildError> {
    if let Some(address) = address {
        PrometheusBuilder::new()
            .with_http_listener(address)
            .idle_timeout(MetricKindMask::ALL, Some(METRICS_IDLE_TIMEOUT))
            .install()?;
        return Ok(true);
    }
    Ok(false)
}

fn install_tracing(
    config: &ObservabilityConfig,
    writer: Option<NonBlocking>,
) -> Result<Option<foreground_logs::ForegroundLogs>, InitError> {
    let fmt_layer: Option<Box<dyn Layer<Registry> + Send + Sync>> = writer.map(|writer| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_error| EnvFilter::new(config.default_log_filter));
        match config.log_format {
            LogFormat::Compact => tracing_subscriber::fmt::layer()
                .compact()
                .with_ansi(true)
                .with_writer(writer)
                .with_filter(filter)
                .boxed(),
            LogFormat::Pretty => tracing_subscriber::fmt::layer()
                .pretty()
                .with_ansi(true)
                .with_writer(writer)
                .with_filter(filter)
                .boxed(),
        }
    });
    let (foreground_layer, foreground_logs) = config
        .foreground_log_capacity
        .map(|capacity| foreground_logs::channel(capacity, config.foreground_log_level))
        .map_or((None, None), |(layer, logs)| (Some(layer), Some(logs)));
    let subscriber = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(foreground_layer);
    #[cfg(feature = "profile-with-tracy")]
    let subscriber = subscriber
        .with(tracing_tracy::TracyLayer::default().with_filter(filter_fn(is_tracy_signal)));
    subscriber
        .try_init()
        .map_err(InitError::Tracing)
        .map(|()| foreground_logs)
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
    metrics::describe_counter!(
        "blackflower_observability_foreground_log_lines_dropped_total",
        metrics::Unit::Count,
        "Structured foreground log records dropped by the bounded capture queue",
    );
    host_metrics::describe_metrics();
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
