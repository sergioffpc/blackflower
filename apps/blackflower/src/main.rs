use std::io::IsTerminal as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context as _, Result, bail};
use blackflower::foreground::{self, ClientCapabilities, ForegroundConfig};
use blackflower_observability::{ObservabilityConfig, ObservabilityGuard, init};
use clap::Parser;

const FOREGROUND_LOG_CAPACITY: usize = 4_096;
const DEFAULT_METRICS_PORT: u16 = 9_002;

#[derive(Debug, Parser)]
#[command(version, about = "Blackflower native client")]
struct Arguments {
    /// Run the native client with an interactive terminal dashboard.
    #[arg(long)]
    foreground: bool,

    /// Loopback address for client metrics and foreground polling.
    #[arg(long, default_value_t = default_metrics_address(), requires = "foreground")]
    metrics_bind_address: SocketAddr,
}

fn main() -> Result<()> {
    let arguments = Arguments::parse();
    validate_arguments(&arguments)?;

    let mut config = ObservabilityConfig::client(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    if arguments.foreground {
        let capacity = NonZeroUsize::new(FOREGROUND_LOG_CAPACITY)
            .context("foreground log capacity must be non-zero")?;
        config = config
            .with_metrics_bind_address(Some(arguments.metrics_bind_address))
            .with_host_metrics(true)
            .with_foreground_logs(Default::default(), capacity);
    }
    let mut observability = init(&config).context("observability init failed")?;
    observability.report_health();

    if arguments.foreground {
        run_with_foreground(&config, &mut observability)?;
    } else {
        blackflower::run().context("client application failed")?;
    }

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

fn run_with_foreground(
    config: &ObservabilityConfig,
    observability: &mut ObservabilityGuard,
) -> Result<()> {
    let metrics_address = config
        .metrics_bind_address()
        .context("client foreground metrics endpoint is disabled")?;
    let (log_receiver, log_control) = observability
        .take_foreground_logs()
        .context("client foreground log capture is disabled")?;
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let foreground_shutdown = Arc::clone(&shutdown_requested);
    let foreground_config = ForegroundConfig {
        service_name: config.service_name(),
        service_version: env!("CARGO_PKG_VERSION"),
        metrics_address,
        log_receiver,
        log_control,
        capabilities: ClientCapabilities::shell(),
        shutdown_requested: Arc::clone(&foreground_shutdown),
    };
    let foreground_thread = std::thread::Builder::new()
        .name("blackflower-client-foreground".to_owned())
        .spawn(move || {
            let result = foreground::run(foreground_config);
            foreground_shutdown.store(true, Ordering::Release);
            result
        })
        .context("client foreground thread startup failed")?;

    let client_result = blackflower::run_with_shutdown(Arc::clone(&shutdown_requested));
    shutdown_requested.store(true, Ordering::Release);
    let foreground_result = foreground_thread
        .join()
        .map_err(|_panic| anyhow::anyhow!("client foreground thread panicked"))?;
    client_result.context("client application failed")?;
    foreground_result.context("client foreground failed")
}

const fn default_metrics_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_METRICS_PORT)
}

#[cfg(test)]
#[path = "../tests/unit/arguments.rs"]
mod tests;
