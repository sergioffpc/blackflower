use std::time::Duration;

use blackflower_harness::{
    ClientHarness, ClientHarnessConfig, ClientHarnessError, ClientPrediction, ClientView,
};
use blackflower_navigation::{NavMesh, NavMeshAsset, Query, QueryFilter};
use blackflower_networking::SimulationTick;
use blackflower_networking_quic::{
    ClientEndpointConfig, ClientNetworkHandle, QuicClient, QuicError,
};

use crate::{AgentDiagnosticConfig, AgentDiagnostics};

/// Complete existing-system inputs required to establish one ordinary agent client.
pub struct AgentRuntimeConfig {
    endpoint: ClientEndpointConfig,
    harness: ClientHarnessConfig,
    navigation: NavMeshAsset,
    diagnostics: Option<AgentDiagnosticConfig>,
}

impl AgentRuntimeConfig {
    /// Compose validated transport, session, and cooked navigation inputs.
    #[must_use]
    pub const fn new(
        endpoint: ClientEndpointConfig,
        harness: ClientHarnessConfig,
        navigation: NavMeshAsset,
    ) -> Self {
        Self {
            endpoint,
            harness,
            navigation,
            diagnostics: None,
        }
    }

    /// Attach a real controller identity and optional bounded foreground observer.
    #[must_use]
    pub fn with_diagnostics(mut self, diagnostics: AgentDiagnosticConfig) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }
}

/// Established headless client systems available to a future agent controller.
///
/// The runtime deliberately has no policy callback or autonomous tick loop. A
/// gameplay layer reads [`Self::view`], uses [`Self::navigation_query`] when it
/// needs a path, and submits validated controls through [`Self::harness_mut`].
pub struct AgentRuntime<P>
where
    P: ClientPrediction,
{
    endpoint: QuicClient,
    harness: ClientHarness<ClientNetworkHandle, P>,
    navigation: NavMesh,
    navigation_filter: QueryFilter,
    diagnostics: AgentDiagnostics,
}

impl<P> AgentRuntime<P>
where
    P: ClientPrediction,
{
    /// Validate navigation, establish QUIC, and start the shared client harness.
    pub async fn connect(
        config: AgentRuntimeConfig,
        prediction: P,
    ) -> Result<Self, AgentRuntimeError<P::Error>> {
        let navigation_filter = config.navigation.query_filter()?;
        let navigation = config.navigation.instantiate()?;
        let endpoint = QuicClient::bind(config.endpoint)?;
        let connection = endpoint.connect().await?;
        let transport = connection.spawn_io().await?;
        let harness = ClientHarness::new(transport, prediction, config.harness)
            .map_err(AgentRuntimeError::Harness)?;
        let diagnostics =
            AgentDiagnostics::connected(config.diagnostics, harness.view().session_state());
        Ok(Self {
            endpoint,
            harness,
            navigation,
            navigation_filter,
            diagnostics,
        })
    }

    /// Drain bounded client transport work and advance session activation.
    pub fn update(
        &mut self,
        now: Duration,
        authoritative_tick: SimulationTick,
    ) -> Result<(), AgentRuntimeError<P::Error>> {
        self.harness
            .update(now, authoritative_tick)
            .map_err(AgentRuntimeError::Harness)?;
        self.diagnostics
            .set_session_state(self.harness.view().session_state());
        Ok(())
    }

    /// Return the immutable ordinary-client observation boundary.
    #[must_use]
    pub fn view(&self) -> ClientView<'_, P::State> {
        self.harness.view()
    }

    /// Return the shared client harness for connection coordination.
    #[must_use]
    pub const fn harness(&self) -> &ClientHarness<ClientNetworkHandle, P> {
        &self.harness
    }

    /// Return the shared client harness for validated control submission.
    #[must_use]
    pub const fn harness_mut(&mut self) -> &mut ClientHarness<ClientNetworkHandle, P> {
        &mut self.harness
    }

    /// Allocate one Detour query borrowing this runtime's cooked navigation mesh.
    pub fn navigation_query(&self) -> Result<Query<'_>, blackflower_navigation::Error> {
        self.navigation.query()
    }

    /// Return the cooked, gameplay-authored Detour query filter.
    #[must_use]
    pub const fn navigation_filter(&self) -> &QueryFilter {
        &self.navigation_filter
    }

    /// Return runtime-owned aggregate telemetry and diagnostic publication.
    #[must_use]
    pub const fn diagnostics(&self) -> &AgentDiagnostics {
        &self.diagnostics
    }

    /// Return mutable runtime-owned telemetry for real controller records.
    #[must_use]
    pub const fn diagnostics_mut(&mut self) -> &mut AgentDiagnostics {
        &mut self.diagnostics
    }

    /// Close the owned QUIC endpoint and every connection it owns.
    pub fn close(&self, code: u32, reason: &[u8]) {
        self.endpoint.close(code, reason);
    }
}

/// Failure while composing or servicing the existing agent runtime systems.
#[derive(Debug, thiserror::Error)]
pub enum AgentRuntimeError<PredictionError>
where
    PredictionError: std::error::Error + Send + Sync + 'static,
{
    /// Cooked navigation could not be instantiated or queried.
    #[error("agent navigation initialization failed")]
    Navigation(#[from] blackflower_navigation::Error),
    /// QUIC endpoint setup, connection, or I/O startup failed.
    #[error("agent transport initialization failed")]
    Transport(#[from] QuicError),
    /// The shared ordinary-client session failed.
    #[error("agent client harness failed")]
    Harness(#[source] ClientHarnessError<QuicError, PredictionError>),
}
