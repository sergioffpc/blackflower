//! Native Blackflower client shell.
//!
//! This crate owns process-facing window and device-input lifecycle plus the
//! client-only presentation loop. Gameplay can compose an already established
//! shared harness through [`run_with_harness`]; transport construction,
//! prediction policy, and renderer submission remain external boundaries.

mod application;
pub mod input;
pub mod lifecycle;
mod runtime;

use anyhow::{Context as _, Result};
use application::ClientApplication;
use blackflower_harness::{ClientHarness, ClientPrediction, ClientTransport};
pub use runtime::{HarnessPresentationRuntime, PresentationBridge};
use winit::event_loop::{ControlFlow, EventLoop};

/// Run the native client event loop on the calling thread.
pub fn run() -> Result<()> {
    run_application(ClientApplication::new()?)
}

/// Run the native client with an already established shared harness.
pub fn run_with_harness<T, P, B>(harness: ClientHarness<T, P>, bridge: B) -> Result<()>
where
    T: ClientTransport + 'static,
    P: ClientPrediction + 'static,
    B: PresentationBridge<P::State> + 'static,
{
    let runtime = HarnessPresentationRuntime::new(harness, bridge)?;
    run_application(ClientApplication::with_runtime(Box::new(runtime))?)
}

fn run_application(mut application: ClientApplication) -> Result<()> {
    let event_loop = EventLoop::new().context("native event loop creation failed")?;
    event_loop.set_control_flow(ControlFlow::Wait);

    event_loop
        .run_app(&mut application)
        .context("native event loop failed")?;
    application.finish()
}
