use std::convert::Infallible;
use std::time::Duration;

use anyhow::Result;
use blackflower_assets::AssetStore;
use blackflower_ecs::TickDelta;
use blackflower_harness::{
    ClientEvent, ClientHarness, ClientHarnessConfig, ClientPrediction, ClientView, PredictionUpdate,
};
use blackflower_networking::{ControlFrame, SimulationTick};
use blackflower_networking_protocol::v1::ProtocolComponent;
use blackflower_networking_quic::{
    ClientEndpointConfig, ClientNetworkHandle, QuicClient, QuicError,
};
use blackflower_networking_replication::Snapshot;
use blackflower_world_presentation::{FrameIndex, PresentationWorld};

use crate::runtime::{ApplicationRuntime, HarnessPresentationRuntime, PresentationBridge};

/// Complete transport and session inputs for the native network client.
pub struct ClientConnectionConfig {
    /// QUIC address, service name, and exact service-CA trust roots.
    pub endpoint: ClientEndpointConfig,
    /// Compiled protocol revision and locally derived signed content identity.
    pub harness: ClientHarnessConfig,
    /// Locally verified signed assets retained for the selected map lifetime.
    pub installed_assets: AssetStore,
}

/// Established network client kept alive beside the native event loop.
pub struct ConnectedClient {
    _endpoint: QuicClient,
    _installed_assets: AssetStore,
    runtime: HarnessPresentationRuntime<
        ClientNetworkHandle,
        SnapshotPrediction,
        NetworkPresentationBridge,
    >,
}

impl ConnectedClient {
    /// Establish authenticated QUIC and start the shared client harness.
    pub async fn connect(config: ClientConnectionConfig) -> Result<Self, ClientConnectionError> {
        let endpoint = QuicClient::bind(config.endpoint)?;
        let connection = endpoint.connect().await?;
        let transport = connection.spawn_io().await?;
        let harness = ClientHarness::new(transport, SnapshotPrediction::default(), config.harness)
            .map_err(ClientConnectionError::Harness)?;
        let runtime = HarnessPresentationRuntime::new(harness, NetworkPresentationBridge)
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
struct SnapshotPrediction {
    tick: SimulationTick,
    state: Option<Snapshot>,
}

impl ClientPrediction for SnapshotPrediction {
    type State = Snapshot;
    type Error = SnapshotPredictionError;

    fn current_tick(&self) -> SimulationTick {
        self.tick
    }

    fn bootstrap(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        validate_snapshot(snapshot)?;
        let tick = SimulationTick::new(snapshot.tick().get());
        self.tick = tick;
        self.state = Some(snapshot.clone());
        Ok(PredictionUpdate::Bootstrapped { tick })
    }

    fn apply_snapshot(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        validate_snapshot(snapshot)?;
        let tick = SimulationTick::new(snapshot.tick().get());
        self.tick = self.tick.max(tick);
        self.state = Some(snapshot.clone());
        Ok(PredictionUpdate::Converged { tick })
    }

    fn queue_control(&mut self, _frame: &ControlFrame) -> Result<(), Self::Error> {
        Err(SnapshotPredictionError::PredictionUnavailable)
    }

    fn advance_to(&mut self, target: SimulationTick) -> Result<(), Self::Error> {
        self.tick = self.tick.max(target);
        Ok(())
    }

    fn predicted_state(&self) -> Option<&Self::State> {
        self.state.as_ref()
    }
}

struct NetworkPresentationBridge;

impl PresentationBridge<Snapshot> for NetworkPresentationBridge {
    type Error = Infallible;

    fn capture(
        &mut self,
        _presentation: &mut PresentationWorld,
        view: ClientView<'_, Snapshot>,
        events: &[ClientEvent],
    ) -> Result<(), Self::Error> {
        for event in events {
            log_client_event(event);
        }
        let _latest_authoritative = view.authoritative();
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
        ClientEvent::ControlBound(binding) => tracing::info!(
            target: "blackflower_client",
            event_name = "network_control_bound",
            control_epoch = binding.control_epoch,
            controlled_entity = binding.controlled_entity.get(),
            "server assigned the controlled actor",
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

/// Failure while establishing the native network client.
#[derive(Debug, thiserror::Error)]
pub enum ClientConnectionError {
    /// QUIC endpoint setup, handshake, or bounded task startup failed.
    #[error(transparent)]
    Transport(#[from] QuicError),
    /// Shared client session initialization failed.
    #[error("client harness initialization failed")]
    Harness(#[source] blackflower_harness::ClientHarnessError<QuicError, SnapshotPredictionError>),
    /// Presentation-world composition failed.
    #[error("connected client composition failed")]
    Composition(#[source] anyhow::Error),
}

/// Snapshot validation failure before the real prediction driver is installed.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotPredictionError {
    /// A component does not belong to the negotiated v1 schema or is non-canonical.
    #[error("authoritative snapshot contains an invalid v1 component")]
    InvalidComponent(#[source] blackflower_networking_protocol::v1::ProtocolError),
    /// Local forward prediction is not installed yet.
    #[error("local forward prediction is not installed")]
    PredictionUnavailable,
}

fn validate_snapshot(snapshot: &Snapshot) -> Result<(), SnapshotPredictionError> {
    for (_entity, state) in snapshot.entities() {
        for (id, component) in state.components() {
            ProtocolComponent::decode(id, component.bytes())
                .map_err(SnapshotPredictionError::InvalidComponent)?;
        }
    }
    Ok(())
}
