use std::io::IsTerminal as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context as _, Result, bail};
use blackflower_agent::foreground::{self, AgentCapabilities, ForegroundConfig};
use blackflower_observability::{
    ForegroundLogLevel, ObservabilityConfig, ObservabilityGuard, init,
};
use clap::Parser;

const FOREGROUND_LOG_CAPACITY: usize = 4_096;
const DEFAULT_METRICS_PORT: u16 = 9_001;

#[derive(Debug, Parser)]
#[command(version, about = "Blackflower headless ordinary-client agent")]
struct Arguments {
    /// Run the interactive foreground metrics and logs dashboard.
    #[arg(long)]
    foreground: bool,

    /// Loopback address for Prometheus metrics and foreground polling.
    #[arg(long, default_value_t = default_metrics_address())]
    metrics_bind_address: SocketAddr,

    /// Initial foreground capture and view log level.
    #[arg(
        long,
        default_value = "info",
        value_parser = parse_log_level,
        requires = "foreground"
    )]
    log_level: ForegroundLogLevel,

    /// Initial regex over structured log target, message, and fields.
    #[arg(long, requires = "foreground")]
    log_regex: Option<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let arguments = Arguments::parse();
    validate_arguments(&arguments)?;

    let mut config = ObservabilityConfig::client(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        .with_metrics_bind_address(Some(arguments.metrics_bind_address))
        .with_host_metrics(true);
    if arguments.foreground {
        let capacity = NonZeroUsize::new(FOREGROUND_LOG_CAPACITY)
            .context("foreground log capacity must be non-zero")?;
        config = config.with_foreground_logs(arguments.log_level, capacity);
    }
    let mut observability = init(&config).context("observability init failed")?;
    observability.report_health();

    let capabilities = AgentCapabilities::shell();
    tracing::info!(
        target: "blackflower_agent",
        event_name = "agent_shell_started",
        runtime_configured = capabilities.runtime_configured,
        policy_configured = capabilities.policy_configured,
        navigation_loaded = capabilities.navigation_loaded,
        "agent shell started",
    );
    run_application(&arguments, &config, capabilities, &mut observability).await?;
    tracing::info!(
        target: "blackflower_agent",
        event_name = "agent_shell_stopped",
        "agent shell stopped",
    );
    observability.report_health();
    Ok(())
}

fn validate_arguments(arguments: &Arguments) -> Result<()> {
    if !arguments.metrics_bind_address.ip().is_loopback() {
        bail!("--metrics-bind-address must be loopback");
    }
    if arguments.foreground && (!std::io::stdin().is_terminal() || !std::io::stdout().is_terminal())
    {
        bail!("--foreground requires an interactive terminal");
    }
    Ok(())
}

async fn run_application(
    arguments: &Arguments,
    config: &ObservabilityConfig,
    capabilities: AgentCapabilities,
    observability: &mut ObservabilityGuard,
) -> Result<()> {
    if arguments.foreground {
        run_foreground(arguments, config, capabilities, observability).await
    } else {
        shutdown_signal().await
    }
}

async fn run_foreground(
    arguments: &Arguments,
    config: &ObservabilityConfig,
    capabilities: AgentCapabilities,
    observability: &mut ObservabilityGuard,
) -> Result<()> {
    let metrics_address = config
        .metrics_bind_address()
        .context("foreground metrics endpoint is disabled")?;
    let (log_receiver, log_control) = observability
        .take_foreground_logs()
        .context("foreground log capture is disabled")?;
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let foreground_shutdown = Arc::clone(&shutdown_requested);
    let foreground_config = ForegroundConfig {
        service_name: config.service_name(),
        service_version: env!("CARGO_PKG_VERSION"),
        metrics_address,
        log_receiver,
        log_control,
        initial_view_level: arguments.log_level,
        initial_log_regex: arguments.log_regex.clone(),
        capabilities,
        shutdown_requested: foreground_shutdown,
    };
    let mut foreground_task =
        tokio::task::spawn_blocking(move || foreground::run(foreground_config));

    tokio::select! {
        result = &mut foreground_task => result
            .context("foreground task panicked")?
            .context("foreground mode failed"),
        signal_result = shutdown_signal() => {
            shutdown_requested.store(true, Ordering::Release);
            let foreground_result = foreground_task
                .await
                .context("foreground task panicked")?;
            signal_result?;
            foreground_result.context("foreground mode failed")
        }
    }
}

async fn shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .context("failed to install SIGTERM handler")?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result.context("failed to wait for SIGINT"),
            signal = terminate.recv() => signal.context("SIGTERM signal stream closed"),
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .context("failed to wait for shutdown signal")
    }
}

const fn default_metrics_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_METRICS_PORT)
}

fn parse_log_level(value: &str) -> Result<ForegroundLogLevel, String> {
    match value.to_ascii_lowercase().as_str() {
        "off" => Ok(ForegroundLogLevel::Off),
        "error" => Ok(ForegroundLogLevel::Error),
        "warn" => Ok(ForegroundLogLevel::Warn),
        "info" => Ok(ForegroundLogLevel::Info),
        "debug" => Ok(ForegroundLogLevel::Debug),
        "trace" => Ok(ForegroundLogLevel::Trace),
        _ => Err("expected one of: off, error, warn, info, debug, trace".to_owned()),
    }
}

#[cfg(test)]
#[path = "../tests/unit/arguments.rs"]
mod tests;
