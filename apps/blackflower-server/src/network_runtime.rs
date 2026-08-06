use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use blackflower_networking::{
    AdmissionRejectReason, AuthorityError, ClockState, FlowId, SessionControlMessage, SessionState,
    SimulationTick, decode_control_message, decode_datagram, record_clock_sessions,
    record_clock_uncertainty,
};
use blackflower_networking_quic::NetworkEvent;
use blackflower_networking_replication::{Snapshot, SnapshotTick};

use crate::{
    DedicatedServerNetwork, LoopbackSessionAuthority, NetworkPeer, PeerError, SimulationStatus,
};

const SERVICE_INTERVAL: Duration = Duration::from_millis(2);
const MAX_EVENTS_PER_PEER: usize = 128;

/// Executable-level network supervisor for the explicit loopback bootstrap mode.
pub struct ServerNetworkRuntime {
    network: DedicatedServerNetwork<LoopbackSessionAuthority>,
    peers: Vec<PeerRuntime>,
    simulation: SimulationStatus,
    started: Instant,
    clock_metrics: ClockMetrics,
}

impl ServerNetworkRuntime {
    /// Compose the already validated endpoint, local authority, and simulation clock.
    #[must_use]
    pub fn new(
        network: DedicatedServerNetwork<LoopbackSessionAuthority>,
        simulation: SimulationStatus,
    ) -> Self {
        let clock_metrics = ClockMetrics::default();
        clock_metrics.publish();
        Self {
            network,
            peers: Vec::new(),
            simulation,
            started: Instant::now(),
            clock_metrics,
        }
    }

    /// Run accepts and every peer state machine until orderly process shutdown.
    pub async fn run(mut self, stop: Arc<AtomicBool>) -> Result<(), ServerNetworkRuntimeError> {
        let local_address = self.network.local_addr()?;
        tracing::info!(
            target: "blackflower_server",
            event_name = "network_listening",
            %local_address,
            "QUIC server listening",
        );
        let mut service = tokio::time::interval(SERVICE_INTERVAL);
        service.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        while !stop.load(Ordering::Acquire) {
            tokio::select! {
                peer = self.network.accept(self.started.elapsed()) => {
                    let peer = peer?;
                    tracing::info!(
                        target: "blackflower_server",
                        event_name = "network_peer_accepted",
                        "address-validated QUIC peer accepted",
                    );
                    self.peers.push(PeerRuntime::new(peer));
                }
                _ = service.tick() => self.service_peers(),
            }
        }
        self.peers.clear();
        tracing::info!(
            target: "blackflower_server",
            event_name = "network_stopped",
            "QUIC server stopped",
        );
        Ok(())
    }

    fn service_peers(&mut self) {
        let now = self.started.elapsed();
        let tick = SimulationTick::new(self.simulation.completed_ticks());
        let mut index = 0;
        while index < self.peers.len() {
            let result = self.peers[index].service(&mut self.network, tick, now);
            if let Err(error) = result {
                tracing::warn!(
                    target: "blackflower_server",
                    event_name = "network_peer_failed",
                    %error,
                    "network peer stopped",
                );
                let _removed = self.peers.swap_remove(index);
            } else {
                index += 1;
            }
        }
        let clock_metrics = ClockMetrics::from_peers(&self.peers);
        if clock_metrics != self.clock_metrics {
            clock_metrics.publish();
            self.clock_metrics = clock_metrics;
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ClockMetrics {
    maximum_uncertainty_ticks: u16,
    synchronized: usize,
    unsynchronized: usize,
}

impl ClockMetrics {
    fn from_peers(peers: &[PeerRuntime]) -> Self {
        let synchronized = peers
            .iter()
            .filter(|peer| peer.clock_uncertainty_ticks.is_some())
            .count();
        let maximum_uncertainty_ticks = peers
            .iter()
            .filter_map(|peer| peer.clock_uncertainty_ticks)
            .max()
            .unwrap_or(0);
        Self {
            maximum_uncertainty_ticks,
            synchronized,
            unsynchronized: peers.len().saturating_sub(synchronized),
        }
    }

    fn publish(self) {
        record_clock_uncertainty(u64::from(self.maximum_uncertainty_ticks));
        record_clock_sessions(ClockState::Synchronized, self.synchronized);
        record_clock_sessions(ClockState::Unsynchronized, self.unsynchronized);
    }
}

struct PeerRuntime {
    peer: NetworkPeer,
    bootstrap_applied: bool,
    clock_uncertainty_ticks: Option<u16>,
    activation_scheduled: bool,
    active_reported: bool,
}

impl PeerRuntime {
    fn new(peer: NetworkPeer) -> Self {
        Self {
            peer,
            bootstrap_applied: false,
            clock_uncertainty_ticks: None,
            activation_scheduled: false,
            active_reported: false,
        }
    }

    fn service(
        &mut self,
        network: &mut DedicatedServerNetwork<LoopbackSessionAuthority>,
        tick: SimulationTick,
        now: Duration,
    ) -> Result<(), ServerNetworkRuntimeError> {
        self.peer.record_metrics();
        for _index in 0..MAX_EVENTS_PER_PEER {
            let Some(event) = self.peer.poll_event()? else {
                break;
            };
            self.handle_event(network, event, tick, now)?;
        }
        self.try_schedule_activation(tick)?;
        if self.activation_scheduled && !self.active_reported && self.peer.advance(tick)? {
            self.active_reported = true;
            tracing::info!(
                target: "blackflower_server",
                event_name = "network_peer_active",
                tick = tick.get(),
                "application session activated",
            );
        }
        Ok(())
    }

    fn handle_event(
        &mut self,
        network: &mut DedicatedServerNetwork<LoopbackSessionAuthority>,
        event: NetworkEvent,
        tick: SimulationTick,
        now: Duration,
    ) -> Result<(), ServerNetworkRuntimeError> {
        match event {
            NetworkEvent::SessionControl(frame) => {
                self.handle_control(network, decode_control_message(&frame)?, tick, now)
            }
            NetworkEvent::Datagram(datagram) => {
                let header = decode_datagram(&datagram)?.header;
                match header.flow {
                    FlowId::TimeSync => self.peer.respond_time_sync(&datagram, now)?,
                    FlowId::Input if self.peer.session().state() == SessionState::Active => {
                        let _ingress = self.peer.ingest_input(&datagram, tick, now, true)?;
                    }
                    FlowId::Input
                    | FlowId::SnapshotDelta
                    | FlowId::SnapshotAppliedAck
                    | FlowId::VoiceCapture
                    | FlowId::VoiceDelivery => {
                        return Err(ServerNetworkRuntimeError::UnexpectedDatagramFlow);
                    }
                }
                Ok(())
            }
            NetworkEvent::PathChanged { previous, current } => {
                self.clock_uncertainty_ticks = None;
                tracing::info!(
                    target: "blackflower_server",
                    event_name = "network_path_changed",
                    %previous,
                    %current,
                    "validated peer path changed",
                );
                Ok(())
            }
            NetworkEvent::TransportStopped => Err(ServerNetworkRuntimeError::TransportStopped),
            NetworkEvent::Bootstrap(_) => Err(ServerNetworkRuntimeError::UnexpectedBootstrap),
        }
    }

    fn handle_control(
        &mut self,
        network: &mut DedicatedServerNetwork<LoopbackSessionAuthority>,
        message: SessionControlMessage,
        tick: SimulationTick,
        now: Duration,
    ) -> Result<(), ServerNetworkRuntimeError> {
        match message {
            SessionControlMessage::AdmissionRequest { protocol_revision } => {
                self.negotiate(network, protocol_revision, now)
            }
            SessionControlMessage::ContentReady(content) => {
                network.content_ready(&mut self.peer, &content)?;
                self.queue_empty_bootstrap(tick)
            }
            SessionControlMessage::ContentRejected(reason) => {
                Err(ServerNetworkRuntimeError::ContentRejected(reason))
            }
            SessionControlMessage::BootstrapApplied {
                bootstrap_id,
                snapshot_tick,
                digest,
            } => {
                self.peer
                    .bootstrap_applied(bootstrap_id, snapshot_tick, digest)?;
                self.bootstrap_applied = true;
                Ok(())
            }
            SessionControlMessage::ClockSynchronized { uncertainty_ticks } => {
                if uncertainty_ticks > 2 {
                    return Err(ServerNetworkRuntimeError::ClockNotReady { uncertainty_ticks });
                }
                self.clock_uncertainty_ticks = Some(uncertainty_ticks);
                Ok(())
            }
            SessionControlMessage::ResyncRequest { .. } => {
                self.peer.begin_resync(now)?;
                self.restart_synchronization(tick)
            }
            SessionControlMessage::ResumeRequest { token } => {
                let _resumed = network.resume(&mut self.peer, &token, now)?;
                self.restart_synchronization(tick)
            }
            SessionControlMessage::AdmissionAccepted { .. }
            | SessionControlMessage::AdmissionRejected(_)
            | SessionControlMessage::ContentManifest(_)
            | SessionControlMessage::BootstrapOffer { .. }
            | SessionControlMessage::ActivateAt { .. }
            | SessionControlMessage::ResumeIssued { .. }
            | SessionControlMessage::CommandDisposition { .. }
            | SessionControlMessage::Closing { .. } => {
                Err(ServerNetworkRuntimeError::UnexpectedControlMessage)
            }
        }
    }

    fn negotiate(
        &mut self,
        network: &mut DedicatedServerNetwork<LoopbackSessionAuthority>,
        protocol_revision: blackflower_networking::ProtocolRevision,
        now: Duration,
    ) -> Result<(), ServerNetworkRuntimeError> {
        if let Err(error) = network.admit(&mut self.peer, protocol_revision, now) {
            let reason = admission_rejection(&error);
            self.peer.reject_admission(reason)?;
            return Err(ServerNetworkRuntimeError::Peer(error));
        }
        Ok(())
    }

    fn restart_synchronization(
        &mut self,
        tick: SimulationTick,
    ) -> Result<(), ServerNetworkRuntimeError> {
        self.bootstrap_applied = false;
        self.clock_uncertainty_ticks = None;
        self.activation_scheduled = false;
        self.active_reported = false;
        self.queue_empty_bootstrap(tick)
    }

    fn queue_empty_bootstrap(
        &mut self,
        tick: SimulationTick,
    ) -> Result<(), ServerNetworkRuntimeError> {
        let snapshot = Snapshot::new(SnapshotTick::new(tick.get()), [])?;
        let _bootstrap = self.peer.queue_bootstrap(snapshot)?;
        Ok(())
    }

    fn try_schedule_activation(&mut self, tick: SimulationTick) -> Result<(), PeerError> {
        if self.bootstrap_applied
            && !self.activation_scheduled
            && self.peer.session().state() == SessionState::Synchronizing
            && let Some(uncertainty) = self.clock_uncertainty_ticks
        {
            let scheduled = self
                .peer
                .schedule_activation(tick, u64::from(uncertainty))?;
            self.activation_scheduled = true;
            tracing::info!(
                target: "blackflower_server",
                event_name = "network_peer_activation_scheduled",
                tick = scheduled.get(),
                "application session activation scheduled",
            );
        }
        Ok(())
    }
}

/// Executable network-supervision failure.
#[derive(Debug, thiserror::Error)]
pub enum ServerNetworkRuntimeError {
    /// Per-peer QUIC, authority, session, or replication work failed.
    #[error(transparent)]
    Peer(#[from] PeerError),
    /// Canonical empty snapshot construction failed.
    #[error(transparent)]
    Snapshot(#[from] blackflower_networking_replication::SnapshotError),
    /// Session-control decoding failed.
    #[error(transparent)]
    Wire(#[from] blackflower_networking::WireError),
    /// Client clock did not satisfy the activation threshold.
    #[error("client clock uncertainty is {uncertainty_ticks} ticks")]
    ClockNotReady {
        /// Client-observed uncertainty rounded upward to ticks.
        uncertainty_ticks: u16,
    },
    /// A transport stopped normally or because its bounded I/O task failed.
    #[error("peer transport stopped")]
    TransportStopped,
    /// A client used a server-originated reliable bootstrap stream.
    #[error("client sent an unexpected bootstrap stream")]
    UnexpectedBootstrap,
    /// A client sent a server-originated control message.
    #[error("client sent an unexpected session-control message")]
    UnexpectedControlMessage,
    /// The client does not have the exact signed package set required by the map.
    #[error("client rejected the server map content: {0:?}")]
    ContentRejected(blackflower_networking::ContentRejectReason),
    /// A client used a flow that is not legal in its current lifecycle state.
    #[error("client sent an unexpected datagram flow")]
    UnexpectedDatagramFlow,
}

fn admission_rejection(error: &PeerError) -> AdmissionRejectReason {
    match error {
        PeerError::Session(_) => AdmissionRejectReason::Incompatible,
        PeerError::Authority(
            AuthorityError::Invalid
            | AuthorityError::Expired
            | AuthorityError::Replayed
            | AuthorityError::Wire(_)
            | AuthorityError::Unavailable,
        ) => AdmissionRejectReason::IdentityUnavailable,
        PeerError::Quic(_)
        | PeerError::Wire(_)
        | PeerError::Deduplication(_)
        | PeerError::Baseline(_)
        | PeerError::Delta(_)
        | PeerError::Snapshot(_)
        | PeerError::VoiceDatagram(_)
        | PeerError::Voice(_)
        | PeerError::WrongInputFlow
        | PeerError::WrongVoiceFlow
        | PeerError::WrongTimeSyncFlow
        | PeerError::UnexpectedTimeSyncMessage
        | PeerError::UnexpectedBootstrapAck
        | PeerError::ContentMismatch
        | PeerError::EpochExhausted
        | PeerError::FlowSequenceExhausted
        | PeerError::BootstrapIdExhausted
        | PeerError::BandwidthUnavailable
        | PeerError::SharedBudgetUnavailable => AdmissionRejectReason::ProtocolViolation,
    }
}
