//! Native Blackflower network client.
//!
//! This crate owns process-facing window and device-input lifecycle plus the
//! client-only presentation loop. The executable always establishes a network
//! session; gameplay can compose an already established shared harness through
//! [`run_with_harness`]. Prediction policy and renderer submission remain
//! external boundaries.

pub mod foreground;
pub mod input;
pub mod lifecycle;

mod application;
mod connection;
mod runtime;

use anyhow::{Context as _, Result};
use application::ClientApplication;
use blackflower_harness::{ClientHarness, ClientPrediction, ClientTransport};
pub use connection::{ClientConnectionConfig, ClientConnectionError, ConnectedClient};
pub use runtime::{HarnessPresentationRuntime, PresentationBridge};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use winit::event_loop::{ControlFlow, EventLoop};

/// Run an established authoritative-snapshot client on the native event loop.
pub fn run_connected(client: ConnectedClient) -> Result<()> {
    run_application(ClientApplication::with_runtime(Box::new(client), None)?)
}

/// Run an established network client until either native or foreground shutdown.
pub fn run_connected_with_shutdown(
    client: ConnectedClient,
    shutdown_requested: Arc<AtomicBool>,
) -> Result<()> {
    run_application(ClientApplication::with_runtime(
        Box::new(client),
        Some(shutdown_requested),
    )?)
}

/// Run the native client with an already established shared harness.
pub fn run_with_harness<T, P, B>(harness: ClientHarness<T, P>, bridge: B) -> Result<()>
where
    T: ClientTransport + 'static,
    P: ClientPrediction + 'static,
    B: PresentationBridge<P::State> + 'static,
{
    let runtime = HarnessPresentationRuntime::new(harness, bridge)?;
    run_application(ClientApplication::with_runtime(Box::new(runtime), None)?)
}

fn run_application(mut application: ClientApplication) -> Result<()> {
    let event_loop = EventLoop::new().context("native event loop creation failed")?;
    event_loop.set_control_flow(ControlFlow::Wait);

    event_loop
        .run_app(&mut application)
        .context("native event loop failed")?;
    application.finish()
}
