use std::io::IsTerminal as _;
use std::num::NonZeroUsize;

use anyhow::{Context as _, Result, bail};
use blackflower_observability::{ForegroundLogLevel, ObservabilityConfig, init};
use blackflower_server::foreground::{self, ForegroundConfig};
use clap::Parser;

const FOREGROUND_LOG_CAPACITY: usize = 4_096;

#[derive(Debug, Parser)]
#[command(version, about = "Blackflower authoritative server")]
struct Arguments {
    /// Run the interactive Black Ink metrics and logs dashboard.
    #[arg(long)]
    foreground: bool,

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

fn main() -> Result<()> {
    let arguments = Arguments::parse();
    if arguments.foreground && (!std::io::stdin().is_terminal() || !std::io::stdout().is_terminal())
    {
        bail!("--foreground requires an interactive terminal");
    }

    let mut config = ObservabilityConfig::server(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    if arguments.foreground {
        let capacity = NonZeroUsize::new(FOREGROUND_LOG_CAPACITY)
            .context("foreground log capacity must be non-zero")?;
        config = config.with_foreground_logs(arguments.log_level, capacity);
    }
    let metrics_address = config.metrics_bind_address();
    let mut observability = init(&config).context("observability init failed")?;
    observability.report_health();

    if arguments.foreground {
        let metrics_address = metrics_address.context("foreground metrics endpoint is disabled")?;
        let (log_receiver, log_control) = observability
            .take_foreground_logs()
            .context("foreground log capture is disabled")?;
        foreground::run(ForegroundConfig {
            service_name: config.service_name(),
            service_version: env!("CARGO_PKG_VERSION"),
            metrics_address,
            log_receiver,
            log_control,
            initial_view_level: arguments.log_level,
            initial_log_regex: arguments.log_regex,
        })
        .context("foreground mode failed")?;
        observability.report_health();
    }
    Ok(())
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
