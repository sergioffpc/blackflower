//! Interactive foreground diagnostics backed by the Prometheus HTTP endpoint.

mod app;
mod logs;
mod metrics;
mod render;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;

use blackflower_observability::{ForegroundLogControl, ForegroundLogEvent, ForegroundLogLevel};

pub use app::ForegroundError;

/// Inputs required by the interactive foreground diagnostics loop.
pub struct ForegroundConfig {
    /// Executable service name.
    pub service_name: &'static str,
    /// Executable service version.
    pub service_version: &'static str,
    /// Address of the embedded Prometheus HTTP listener.
    pub metrics_address: SocketAddr,
    /// Single structured tracing receiver owned by this UI.
    pub log_receiver: Receiver<ForegroundLogEvent>,
    /// Dynamic foreground capture control.
    pub log_control: ForegroundLogControl,
    /// Initial level applied to the log view.
    pub initial_view_level: ForegroundLogLevel,
    /// Optional initial regex applied to target, message, and fields.
    pub initial_log_regex: Option<String>,
    /// Process-level shutdown request shared with the terminal loop.
    pub shutdown_requested: Arc<AtomicBool>,
}

/// Run the foreground terminal UI until the operator quits.
pub fn run(config: ForegroundConfig) -> Result<(), ForegroundError> {
    let mut app = app::App::new(config)?;
    ratatui::run(|terminal| app.run(terminal)).map_err(ForegroundError::Terminal)
}
