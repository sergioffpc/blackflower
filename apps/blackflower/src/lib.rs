//! Native Blackflower client shell.
//!
//! This crate owns process-facing window and device-input lifecycle. Networking,
//! prediction, presentation, and rendering remain independent runtime layers.

mod application;
pub mod input;
pub mod lifecycle;

use anyhow::{Context as _, Result};
use application::ClientApplication;
use winit::event_loop::{ControlFlow, EventLoop};

/// Run the native client event loop on the calling thread.
pub fn run() -> Result<()> {
    let event_loop = EventLoop::new().context("native event loop creation failed")?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut application = ClientApplication::new().context("client shell creation failed")?;
    event_loop
        .run_app(&mut application)
        .context("native event loop failed")?;
    application.finish()
}
