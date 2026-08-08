//! Interactive foreground diagnostics for the headless agent process.

mod app;
mod render;

use std::net::SocketAddr;
use std::sync::mpsc::Receiver;

use blackflower_observability::{ForegroundLogControl, ForegroundLogEvent};
use blackflower_process::ShutdownToken;

use crate::AgentDiagnosticReceiver;

pub use app::ForegroundError;

/// Static status of the deliberately incomplete agent shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentCapabilities {
    /// Whether deployment inputs have established an [`crate::AgentRuntime`].
    pub runtime_configured: bool,
    /// Whether a gameplay observation encoder and policy are installed.
    pub policy_configured: bool,
    /// Whether a cooked navigation asset is loaded into Detour.
    pub navigation_loaded: bool,
    /// RecastNavigation library version compiled into the runtime.
    pub recastnavigation_version: (u32, u32, u32),
    /// Detour navmesh data version accepted by the runtime.
    pub detour_navmesh_version: u32,
}

impl AgentCapabilities {
    /// Describe the process-only shell before deployment and gameplay wiring.
    #[must_use]
    pub fn shell() -> Self {
        Self {
            runtime_configured: false,
            policy_configured: false,
            navigation_loaded: false,
            recastnavigation_version: blackflower_navigation::recastnavigation_version(),
            detour_navmesh_version: blackflower_navigation::detour_navmesh_version(),
        }
    }
}

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
    /// Current process composition visible to the operator.
    pub capabilities: AgentCapabilities,
    /// Real-runtime diagnostic records, absent from the process-only shell.
    pub diagnostics: Option<AgentDiagnosticReceiver>,
    /// Process-level shutdown request shared with the terminal loop.
    pub shutdown_requested: ShutdownToken,
}

/// Run the foreground terminal UI until the operator quits.
pub fn run(config: ForegroundConfig) -> Result<(), ForegroundError> {
    let mut app = app::App::new(config)?;
    blackflower_observability_tui::run(|terminal| app.run(terminal))
        .map_err(ForegroundError::Terminal)
}
