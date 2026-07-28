use anyhow::{Context as _, Result};
use blackflower_observability::{ObservabilityConfig, init};

fn main() -> Result<()> {
    let observability = init(&ObservabilityConfig::server(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    ))
    .context("observability init failed")?;
    observability.report_health();
    Ok(())
}
