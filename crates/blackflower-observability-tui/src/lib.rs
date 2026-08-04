//! Shared bounded state for Blackflower terminal observability dashboards.
//!
//! Executables own their Ratatui pages and metric mappings. This sibling of
//! `blackflower-observability` owns the reusable structured-log buffer/filter
//! and the asynchronous reader for the process-local Prometheus HTTP endpoint,
//! keeping interactive terminal concerns out of core process instrumentation.

mod logs;
mod metrics;

pub use logs::{FilterEditor, LogState};
pub use metrics::{MetricStore, MetricsPoller, Sample, ScrapeResult};

/// Initialize the terminal backend, run one dashboard, and restore the terminal.
pub fn run(
    application: impl FnOnce(&mut ratatui::DefaultTerminal) -> std::io::Result<()>,
) -> std::io::Result<()> {
    ratatui::run(application)
}
