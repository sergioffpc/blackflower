use std::convert::Infallible;
use std::time::Duration;

use anyhow::Result;
use blackflower_assets::AssetStore;
use blackflower_ecs::TickDelta;
use blackflower_harness::{
    ClientEvent, ClientHarness, ClientHarnessConfig, ClientPrediction, ClientView, PredictionUpdate,
};
use blackflower_networking::{ControlFrame, SimulationTick};
use blackflower_networking_quic::{
    ClientEndpointConfig, ClientNetworkHandle, QuicClient, QuicError,
};
use blackflower_networking_replication::Snapshot;
use blackflower_world_presentation::{FrameIndex, PresentationWorld};

use crate::runtime::{ApplicationRuntime, HarnessPresentationRuntime, PresentationBridge};

/// Complete transport and session inputs for the bootstrap-only native client.
pub struct ClientConnectionConfig {
    /// QUIC address, service name, and exact service-CA trust roots.
    pub endpoint: ClientEndpointConfig,
    /// Compiled protocol revision and locally derived signed content identity.
    pub harness: ClientHarnessConfig,
    /// Locally verified signed assets retained for the selected map lifetime.
    pub installed_assets: AssetStore,
}

/// Established bootstrap-only client kept alive beside the native event loop.
pub struct ConnectedClient {
    _endpoint: QuicClient,
    _installed_assets: AssetStore,
    runtime: HarnessPresentationRuntime<
        ClientNetworkHandle,
        BootstrapPrediction,
        BootstrapPresentationBridge,
    >,
}

impl ConnectedClient {
    /// Establish authenticated QUIC and start the shared client harness.
    pub async fn connect(config: ClientConnectionConfig) -> Result<Self, ClientConnectionError> {
        let endpoint = QuicClient::bind(config.endpoint)?;
        let connection = endpoint.connect().await?;
        let transport = connection.spawn_io().await?;
        let harness = ClientHarness::new(transport, BootstrapPrediction::default(), config.harness)
            .map_err(ClientConnectionError::Harness)?;
        let runtime = HarnessPresentationRuntime::new(harness, BootstrapPresentationBridge)
            .map_err(ClientConnectionError::Composition)?;
        Ok(Self {
            _endpoint: endpoint,
            _installed_assets: config.installed_assets,
            runtime,
        })
    }
}

impl ApplicationRuntime for ConnectedClient {
    fn frame(&mut self, now: Duration, delta: TickDelta) -> Result<bool> {
        self.runtime.frame(now, delta)
    }

    fn current_frame(&self) -> FrameIndex {
        self.runtime.presentation().current_frame()
    }
}

#[derive(Debug, Default)]
struct BootstrapPrediction {
    state: Option<BootstrapState>,
}

impl ClientPrediction for BootstrapPrediction {
    type State = BootstrapState;
    type Error = BootstrapPredictionError;

    fn current_tick(&self) -> SimulationTick {
        self.state
            .map_or(SimulationTick::new(0), |state| state.tick)
    }

    fn bootstrap(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        ensure_empty_snapshot(snapshot)?;
        let tick = SimulationTick::new(snapshot.tick().get());
        self.state = Some(BootstrapState { tick });
        Ok(PredictionUpdate::Bootstrapped { tick })
    }

    fn apply_snapshot(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        ensure_empty_snapshot(snapshot)?;
        let tick = SimulationTick::new(snapshot.tick().get());
        self.state = Some(BootstrapState { tick });
        Ok(PredictionUpdate::Converged { tick })
    }

    fn queue_control(&mut self, _frame: &ControlFrame) -> Result<(), Self::Error> {
        Err(BootstrapPredictionError::GameplayUnavailable)
    }

    fn advance_to(&mut self, target: SimulationTick) -> Result<(), Self::Error> {
        if let Some(state) = self.state.as_mut() {
            state.tick = state.tick.max(target);
        }
        Ok(())
    }

    fn predicted_state(&self) -> Option<&Self::State> {
        self.state.as_ref()
    }
}

#[derive(Debug, Clone, Copy)]
struct BootstrapState {
    tick: SimulationTick,
}

struct BootstrapPresentationBridge;

impl PresentationBridge<BootstrapState> for BootstrapPresentationBridge {
    type Error = Infallible;

    fn capture(
        &mut self,
        _presentation: &mut PresentationWorld,
        view: ClientView<'_, BootstrapState>,
        events: &[ClientEvent],
    ) -> Result<(), Self::Error> {
        for event in events {
            log_client_event(event);
        }
        if view
            .authoritative()
            .is_some_and(|snapshot| !snapshot.is_empty())
        {
            tracing::error!(
                target: "blackflower_client",
                event_name = "bootstrap_projection_rejected",
                "bootstrap-only client received gameplay entities",
            );
        }
        Ok(())
    }
}

fn log_client_event(event: &ClientEvent) {
    match event {
        ClientEvent::ContentReady(content) => tracing::info!(
            target: "blackflower_client",
            event_name = "map_content_ready",
            map_id = %content.map_id,
            content_set_id = %content.required_content_set_id,
            "server-selected map content verified",
        ),
        ClientEvent::ContentRejected {
            required,
            installed,
        } => tracing::error!(
            target: "blackflower_client",
            event_name = "map_content_rejected",
            map_id = %required.map_id,
            required_content_set_id = %required.required_content_set_id,
            installed_content_set_id = %installed,
            "server-selected map content is unavailable",
        ),
        ClientEvent::Activated { tick } => tracing::info!(
            target: "blackflower_client",
            event_name = "network_session_active",
            tick = tick.get(),
            "application session activated",
        ),
        ClientEvent::AdmissionRejected(_)
        | ClientEvent::ResumeIssued { .. }
        | ClientEvent::CommandDisposition { .. }
        | ClientEvent::TimeSync(_)
        | ClientEvent::SnapshotApplied { .. }
        | ClientEvent::VoiceDatagram(_)
        | ClientEvent::PathChanged { .. }
        | ClientEvent::TransportStopped
        | ClientEvent::Closing { .. } => {}
    }
}

/// Failure while establishing the bootstrap-only native client.
#[derive(Debug, thiserror::Error)]
pub enum ClientConnectionError {
    /// QUIC endpoint setup, handshake, or bounded task startup failed.
    #[error(transparent)]
    Transport(#[from] QuicError),
    /// Shared client session initialization failed.
    #[error("client harness initialization failed")]
    Harness(#[source] blackflower_harness::ClientHarnessError<QuicError, BootstrapPredictionError>),
    /// Presentation-world composition failed.
    #[error("connected client composition failed")]
    Composition(#[source] anyhow::Error),
}

/// Bootstrap-only prediction cannot decode gameplay state or controls.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapPredictionError {
    /// Gameplay entities require the concrete gameplay component schema.
    #[error("bootstrap-only prediction received gameplay state")]
    GameplayState,
    /// Controls require the concrete gameplay control schema.
    #[error("bootstrap-only prediction cannot accept gameplay controls")]
    GameplayUnavailable,
}

fn ensure_empty_snapshot(snapshot: &Snapshot) -> Result<(), BootstrapPredictionError> {
    if snapshot.is_empty() {
        Ok(())
    } else {
        Err(BootstrapPredictionError::GameplayState)
    }
}
