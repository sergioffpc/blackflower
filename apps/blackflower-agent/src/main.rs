use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::{Context as _, Result, bail};
use blackflower_agent::foreground::{self, AgentCapabilities, ForegroundConfig};
use blackflower_agent::initialize_agent_metrics;
use blackflower_observability::{ObservabilityConfig, ObservabilityGuard, init};
use blackflower_process::{
    LaunchMode, ShutdownToken, validate_foreground_terminal, wait_for_shutdown_signal,
};
use clap::Parser;

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

fn main() -> Result<()> {
    let arguments = Arguments::parse();
    validate_arguments(&arguments)?;
    if !LaunchMode::from_foreground_flag(arguments.foreground)
        .enter()
        .context("process mode startup failed")?
        .should_run()
    {
        return Ok(());
    }

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create agent runtime")?
        .block_on(run(arguments))
}

async fn run(arguments: Arguments) -> Result<()> {
    let mut config = ObservabilityConfig::client(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        .with_metrics_bind_address(Some(arguments.metrics_bind_address))
        .with_host_metrics(true);
    if arguments.foreground {
        config = config.with_default_foreground_logs();
    }
    let mut observability = init(&config).context("observability init failed")?;
    initialize_agent_metrics();
    observability.report_log_pipeline_health();

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
    observability.report_log_pipeline_health();
    Ok(())
}

fn validate_arguments(arguments: &Arguments) -> Result<()> {
    if !arguments.metrics_bind_address.ip().is_loopback() {
        bail!("--metrics-bind-address must be loopback");
    }
    validate_foreground_terminal(arguments.foreground)?;
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
        wait_for_shutdown_signal()
            .await
            .context("shutdown signal wait failed")?;
        Ok(())
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
    let shutdown_requested = ShutdownToken::new();
    let foreground_config = ForegroundConfig {
        service_name: config.service_name(),
        service_version: env!("CARGO_PKG_VERSION"),
        metrics_address,
        log_receiver,
        log_control,
        capabilities,
        diagnostics: None,
        shutdown_requested: shutdown_requested.clone(),
    };
    let mut foreground_task =
        tokio::task::spawn_blocking(move || foreground::run(foreground_config));

    tokio::select! {
        result = &mut foreground_task => result
            .context("foreground task panicked")?
            .context("foreground mode failed"),
        signal_result = wait_for_shutdown_signal() => {
            shutdown_requested.request();
            let foreground_result = foreground_task
                .await
                .context("foreground task panicked")?;
            signal_result.context("shutdown signal wait failed")?;
            foreground_result.context("foreground mode failed")
        }
    }
}

const fn default_metrics_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_METRICS_PORT)
}
