//! Interactive terminal observability for the native client process.

mod app;
mod render;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;

use blackflower_observability::{ForegroundLogControl, ForegroundLogEvent};

pub use app::ForegroundError;

/// Static composition status of the current client executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientCapabilities {
    /// Whether the native `winit` application loop is part of this process.
    pub native_application: bool,
    /// Whether an established shared client harness was supplied.
    pub session_configured: bool,
    /// Whether the client-only presentation world is running.
    pub presentation_configured: bool,
    /// Whether a renderer backend is submitting presentation frames.
    pub renderer_configured: bool,
}

impl ClientCapabilities {
    /// Describe the bootstrap-only client with an established shared harness.
    #[must_use]
    pub const fn connected() -> Self {
        Self {
            native_application: true,
            session_configured: true,
            presentation_configured: true,
            renderer_configured: false,
        }
    }
}

/// Inputs required by the client terminal dashboard.
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
    /// Current client composition visible to the operator.
    pub capabilities: ClientCapabilities,
    /// Shutdown request shared with the native application loop.
    pub shutdown_requested: Arc<AtomicBool>,
}

/// Run the client terminal dashboard until either side requests shutdown.
pub fn run(config: ForegroundConfig) -> Result<(), ForegroundError> {
    let mut app = app::App::new(config)?;
    blackflower_observability_tui::run(|terminal| app.run(terminal))
        .map_err(ForegroundError::Terminal)
}
