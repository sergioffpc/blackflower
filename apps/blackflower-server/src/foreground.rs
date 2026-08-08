//! Interactive foreground diagnostics backed by the Prometheus HTTP endpoint.

mod app;
mod render;

use std::net::SocketAddr;
use std::sync::mpsc::Receiver;

use blackflower_observability::{ForegroundLogControl, ForegroundLogEvent};
use blackflower_process::ShutdownToken;

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
    /// Process-level shutdown request shared with the terminal loop.
    pub shutdown_requested: ShutdownToken,
}

/// Run the foreground terminal UI until the operator quits.
pub fn run(config: ForegroundConfig) -> Result<(), ForegroundError> {
    let mut app = app::App::new(config)?;
    blackflower_observability_tui::run(|terminal| app.run(terminal))
        .map_err(ForegroundError::Terminal)
}
