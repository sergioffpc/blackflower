use std::time::Duration;

use anyhow::{Context as _, Result};
use blackflower_assets::AssetStore;
use blackflower_ecs::TickDelta;
use blackflower_harness::{
    ClientEvent, ClientHarness, ClientHarnessConfig, ClientPrediction as _, ClientView,
    PredictionUpdate,
};
use blackflower_networking::SessionState;
use blackflower_networking_quic::{
    ClientEndpointConfig, ClientNetworkHandle, QuicClient, QuicError,
};
use blackflower_world_presentation::{
    FrameIndex, MovementSampleKind, MovementSourceId, PresentationMovementError,
    PresentationMovementSample, PresentationWorld,
};

use crate::controls::NativeMovementControls;
use crate::input::InputSnapshot;
use crate::prediction::{ClientMovementPrediction, PredictedMovementState};
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
        ClientMovementPrediction,
        NetworkPresentationBridge,
    >,
    controls: NativeMovementControls,
}

impl ConnectedClient {
    /// Establish authenticated QUIC and start the shared client harness.
    pub async fn connect(config: ClientConnectionConfig) -> Result<Self, ClientConnectionError> {
        let endpoint = QuicClient::bind(config.endpoint)?;
        let connection = endpoint.connect().await?;
        let transport = connection.spawn_io().await?;
        let prediction = ClientMovementPrediction::new()
            .map_err(|error| ClientConnectionError::Prediction(anyhow::Error::new(error)))?;
        let harness = ClientHarness::new(transport, prediction, config.harness)
            .map_err(|error| ClientConnectionError::Harness(anyhow::Error::new(error)))?;
        let runtime = HarnessPresentationRuntime::new(harness, NetworkPresentationBridge)
            .map_err(ClientConnectionError::Composition)?;
        Ok(Self {
            _endpoint: endpoint,
            _installed_assets: config.installed_assets,
            runtime,
            controls: NativeMovementControls::default(),
        })
    }

    fn submit_movement_control(&mut self, input: &InputSnapshot) -> Result<()> {
        if self.runtime.harness().view().session_state() != SessionState::Active {
            self.controls.reset();
            return Ok(());
        }
        let current_tick = self.runtime.harness().prediction().current_tick();
        let Some(prepared) = self.controls.prepare(current_tick, input)? else {
            return Ok(());
        };
        let execute_tick = prepared.submission.execute_tick;
        if prepared.reset_timeline {
            self.runtime.harness_mut().reset_control_timeline();
        }
        let _sequence = self
            .runtime
            .harness_mut()
            .submit_control(prepared.submission)
            .context("native movement control submission failed")?;
        self.controls.commit(execute_tick);
        Ok(())
    }
}

impl ApplicationRuntime for ConnectedClient {
    fn frame(&mut self, now: Duration, delta: TickDelta, input: &InputSnapshot) -> Result<bool> {
        let should_continue = self.runtime.frame(now, delta)?;
        if should_continue {
            self.submit_movement_control(input)?;
        }
        Ok(should_continue)
    }

    fn current_frame(&self) -> FrameIndex {
        self.runtime.presentation().current_frame()
    }
}

struct NetworkPresentationBridge;

impl PresentationBridge<PredictedMovementState> for NetworkPresentationBridge {
    type Error = PresentationMovementError;

    fn capture(
        &mut self,
        presentation: &mut PresentationWorld,
        view: ClientView<'_, PredictedMovementState>,
        events: &[ClientEvent],
    ) -> Result<(), Self::Error> {
        for event in events {
            log_client_event(event);
        }
        presentation.set_local_movement_sample(local_movement_sample(view.predicted(), events)?)?;
        Ok(())
    }
}

fn local_movement_sample(
    predicted: Option<&PredictedMovementState>,
    events: &[ClientEvent],
) -> Result<Option<PresentationMovementSample>, PresentationMovementError> {
    let kind = if events.iter().any(|event| {
        matches!(
            event,
            ClientEvent::SnapshotApplied {
                prediction: PredictionUpdate::Bootstrapped { .. },
                ..
            }
        )
    }) {
        MovementSampleKind::Reset
    } else if events.iter().any(|event| {
        matches!(
            event,
            ClientEvent::SnapshotApplied {
                prediction: PredictionUpdate::Reconciled { .. },
                ..
            }
        )
    }) {
        MovementSampleKind::Reconciled
    } else {
        MovementSampleKind::Predicted
    };
    predicted
        .map(|predicted| {
            PresentationMovementSample::new(
                MovementSourceId::new(predicted.controlled_entity.get())?,
                predicted.position_meters,
                predicted.orientation,
                kind,
            )
        })
        .transpose()
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
    Harness(#[source] anyhow::Error),
    /// Concrete movement prediction world initialization failed.
    #[error("client movement prediction initialization failed")]
    Prediction(#[source] anyhow::Error),
    /// Presentation-world composition failed.
    #[error("connected client composition failed")]
    Composition(#[source] anyhow::Error),
}

#[cfg(test)]
#[path = "../tests/unit/connection.rs"]
mod tests;
