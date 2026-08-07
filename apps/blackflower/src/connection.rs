use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context as _, Result};
use blackflower_assets::{AssetId, AssetStore, InvalidAssetId, MapAsset, MapAssetError};
use blackflower_ecs::TickDelta;
use blackflower_harness::{
    ClientEvent, ClientHarness, ClientHarnessConfig, ClientPrediction as _, ClientView,
    PredictionUpdate,
};
use blackflower_networking::SessionState;
use blackflower_networking_quic::{
    ClientEndpointConfig, ClientNetworkHandle, QuicClient, QuicError,
};
use blackflower_rendering::ResourceHandle;
use blackflower_world_presentation::{
    FrameIndex, LocalVisualBinding, MovementSampleKind, MovementSourceId,
    PresentationMovementError, PresentationMovementSample, PresentationSceneError,
    PresentationWorld,
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
        let ClientConnectionConfig {
            endpoint: endpoint_config,
            harness: harness_config,
            installed_assets,
        } = config;
        let endpoint = QuicClient::bind(endpoint_config)?;
        let connection = endpoint.connect().await?;
        let transport = connection.spawn_io().await?;
        let prediction = ClientMovementPrediction::new()
            .map_err(|error| ClientConnectionError::Prediction(anyhow::Error::new(error)))?;
        let harness = ClientHarness::new(transport, prediction, harness_config)
            .map_err(|error| ClientConnectionError::Harness(anyhow::Error::new(error)))?;
        let runtime = HarnessPresentationRuntime::new(
            harness,
            NetworkPresentationBridge::new(installed_assets),
        )
        .map_err(ClientConnectionError::Composition)?;
        Ok(Self {
            _endpoint: endpoint,
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
    fn set_viewport(&mut self, width: u32, height: u32) -> Result<()> {
        self.runtime.set_viewport(width, height)
    }

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

struct NetworkPresentationBridge {
    installed_assets: AssetStore,
    resources: ClientResourceRegistry,
}

impl NetworkPresentationBridge {
    fn new(installed_assets: AssetStore) -> Self {
        Self {
            installed_assets,
            resources: ClientResourceRegistry::default(),
        }
    }

    fn capture_content(
        &mut self,
        presentation: &PresentationWorld,
        event: &ClientEvent,
    ) -> Result<(), NetworkPresentationError> {
        match event {
            ClientEvent::ContentReady(content) => {
                let map_id = AssetId::from_str(content.map_id.as_str())?;
                let map = MapAsset::load(&self.installed_assets, &map_id)?;
                let resource = self.resources.resolve(map.player_model())?;
                presentation.set_local_visual_binding(Some(LocalVisualBinding::new(resource)))?;
            }
            ClientEvent::ContentRejected { .. }
            | ClientEvent::AdmissionRejected(_)
            | ClientEvent::TransportStopped
            | ClientEvent::Closing { .. } => {
                presentation.set_local_visual_binding(None)?;
            }
            ClientEvent::Activated { .. }
            | ClientEvent::ControlBound(_)
            | ClientEvent::ResumeIssued { .. }
            | ClientEvent::CommandDisposition { .. }
            | ClientEvent::TimeSync(_)
            | ClientEvent::SnapshotApplied { .. }
            | ClientEvent::VoiceDatagram(_)
            | ClientEvent::PathChanged { .. } => {}
        }
        Ok(())
    }
}

impl PresentationBridge<PredictedMovementState> for NetworkPresentationBridge {
    type Error = NetworkPresentationError;

    fn capture(
        &mut self,
        presentation: &mut PresentationWorld,
        view: ClientView<'_, PredictedMovementState>,
        events: &[ClientEvent],
    ) -> Result<(), Self::Error> {
        for event in events {
            log_client_event(event);
            self.capture_content(presentation, event)?;
        }
        presentation.set_local_movement_sample(local_movement_sample(view.predicted(), events)?)?;
        Ok(())
    }
}

#[derive(Debug)]
struct ClientResourceRegistry {
    next_handle: u64,
    by_asset: BTreeMap<AssetId, ResourceHandle>,
}

impl Default for ClientResourceRegistry {
    fn default() -> Self {
        Self {
            next_handle: 1,
            by_asset: BTreeMap::new(),
        }
    }
}

impl ClientResourceRegistry {
    fn resolve(&mut self, asset: &AssetId) -> Result<ResourceHandle, NetworkPresentationError> {
        if let Some(handle) = self.by_asset.get(asset) {
            return Ok(*handle);
        }
        if self.next_handle == 0 {
            return Err(NetworkPresentationError::ResourceHandlesExhausted);
        }
        let handle = ResourceHandle::new(self.next_handle);
        self.next_handle = self.next_handle.checked_add(1).unwrap_or(0);
        self.by_asset.insert(asset.clone(), handle);
        Ok(handle)
    }
}

#[derive(Debug, thiserror::Error)]
enum NetworkPresentationError {
    #[error(transparent)]
    InvalidAssetId(#[from] InvalidAssetId),
    #[error(transparent)]
    Map(#[from] MapAssetError),
    #[error(transparent)]
    Movement(#[from] PresentationMovementError),
    #[error(transparent)]
    Scene(#[from] PresentationSceneError),
    #[error("client resource handle space is exhausted")]
    ResourceHandlesExhausted,
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
