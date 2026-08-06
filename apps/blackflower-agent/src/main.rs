use std::io::IsTerminal as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context as _, Result, bail};
use blackflower_agent::foreground::{self, AgentCapabilities, ForegroundConfig};
use blackflower_agent::initialize_agent_metrics;
use blackflower_observability::{ObservabilityConfig, ObservabilityGuard, init};
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
        config = config.with_foreground_logs(Default::default(), capacity);
    }
    let mut observability = init(&config).context("observability init failed")?;
    initialize_agent_metrics();
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
        run_foreground(config, capabilities, observability).await
    } else {
        shutdown_signal().await
    }
}

async fn run_foreground(
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
        capabilities,
        diagnostics: None,
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
