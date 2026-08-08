use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use blackflower_networking::{
    AdmissionRejectReason, AuthorityError, ClockState, ControlBinding, FlowId,
    MAX_FUTURE_COMMAND_TICKS, OperationalState, ProtocolRevision, RECONNECT_WINDOW,
    SessionControlMessage, SessionId, SessionState, SimulationTick, decode_control_message,
    decode_datagram, operational_state, record_clock_sessions, record_clock_uncertainty,
    validate_command_codec, validate_control_codec,
};
use blackflower_networking_protocol::v1::{
    MovementControl as WireMovementControl, MovementControlCodec, NoCommandsCodec,
};
use blackflower_networking_quic::NetworkEvent;
use blackflower_networking_replication::EntityIdAllocator;
use blackflower_world_simulation::{
    ActorId, MovementControl, MovementFrame, SNAPSHOT_INTERVAL_TICKS,
};

use crate::{
    DedicatedServerNetwork, InputIngress, LoopbackSessionAuthority, NetworkPeer, PeerError,
    SimulationStatus, project_movement_frame,
};

const SERVICE_INTERVAL: Duration = Duration::from_millis(2);
const MAX_EVENTS_PER_PEER: usize = 128;

/// Executable-level network supervisor for the explicit loopback bootstrap mode.
pub struct ServerNetworkRuntime {
    network: DedicatedServerNetwork<LoopbackSessionAuthority>,
    peers: Vec<PeerRuntime>,
    detached_sessions: BTreeMap<SessionId, DetachedSession>,
    simulation: SimulationStatus,
    movement_frame: MovementFrame,
    entities: EntityIdAllocator,
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
            detached_sessions: BTreeMap::new(),
            simulation,
            movement_frame: MovementFrame::default(),
            entities: EntityIdAllocator::new(),
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
                    self.peers.push(PeerRuntime::new(peer, self.started.elapsed()));
                }
                _ = service.tick() => self.service_peers()?,
            }
        }
        for peer in self.peers.drain(..) {
            peer.try_despawn(&self.simulation);
        }
        for (_session_id, detached) in std::mem::take(&mut self.detached_sessions) {
            try_despawn_binding(&self.simulation, detached.binding);
        }
        tracing::info!(
            target: "blackflower_server",
            event_name = "network_stopped",
            "QUIC server stopped",
        );
        Ok(())
    }

    fn service_peers(&mut self) -> Result<(), ServerNetworkRuntimeError> {
        let now = self.started.elapsed();
        self.expire_detached_sessions(now);
        if self.peers.is_empty() {
            self.publish_clock_metrics();
            return Ok(());
        }
        if self.simulation.completed_ticks() > self.movement_frame.tick().get() {
            let movement_frame = self.simulation.movement_frame()?;
            if movement_frame.tick() >= self.movement_frame.tick() {
                self.movement_frame = movement_frame;
            }
        }
        let tick = SimulationTick::new(self.movement_frame.tick().get());
        let mut index = 0;
        while index < self.peers.len() {
            if self.service_peer_at(index, tick, now) {
                index += 1;
            }
        }
        self.apply_pending_resumes();
        self.publish_clock_metrics();
        Ok(())
    }

    fn service_peer_at(&mut self, index: usize, tick: SimulationTick, now: Duration) -> bool {
        let result = self.peers[index].service(
            &mut self.network,
            &self.simulation,
            &self.movement_frame,
            &mut self.entities,
            tick,
            now,
        );
        let Err(error) = result else {
            return true;
        };
        if retains_peer(&error) {
            tracing::debug!(
                target: "blackflower_server",
                event_name = "network_peer_throttled",
                %error,
                "network peer work deferred",
            );
            return true;
        }
        tracing::warn!(
            target: "blackflower_server",
            event_name = "network_peer_failed",
            %error,
            "network peer stopped",
        );
        let removed = self.peers.swap_remove(index);
        if preserves_session_for_resume(&error) {
            if let Some((session_id, detached)) = removed.detach(now)
                && let Some(replaced) = self.detached_sessions.insert(session_id, detached)
            {
                try_despawn_binding(&self.simulation, replaced.binding);
            }
        } else {
            removed.try_despawn(&self.simulation);
        }
        false
    }

    fn publish_clock_metrics(&mut self) {
        let clock_metrics = ClockMetrics::from_peers(&self.peers);
        if clock_metrics != self.clock_metrics {
            clock_metrics.publish();
            self.clock_metrics = clock_metrics;
        }
    }

    fn expire_detached_sessions(&mut self, now: Duration) {
        let expired = self
            .detached_sessions
            .iter()
            .filter_map(|(session_id, detached)| {
                (now >= detached.expires_at).then_some(*session_id)
            })
            .collect::<Vec<_>>();
        for session_id in expired {
            if let Some(detached) = self.detached_sessions.remove(&session_id) {
                try_despawn_binding(&self.simulation, detached.binding);
            }
        }
    }

    fn apply_pending_resumes(&mut self) {
        let sessions = self
            .peers
            .iter()
            .filter_map(|peer| peer.pending_replacement)
            .collect::<Vec<_>>();
        for session_id in sessions {
            let binding = self
                .peers
                .iter()
                .position(|peer| {
                    peer.session_id == Some(session_id)
                        && peer.pending_replacement.is_none()
                        && peer.binding.is_some()
                })
                .and_then(|old_index| {
                    let binding = self.peers[old_index].binding.take();
                    drop(self.peers.remove(old_index));
                    binding
                })
                .or_else(|| {
                    self.detached_sessions
                        .remove(&session_id)
                        .map(|detached| detached.binding)
                });
            let Some(new_index) = self
                .peers
                .iter()
                .position(|peer| peer.pending_replacement == Some(session_id))
            else {
                if let Some(binding) = binding {
                    try_despawn_binding(&self.simulation, binding);
                }
                continue;
            };
            if let Some(binding) = binding {
                self.peers[new_index].adopt_resumed_binding(binding);
            } else {
                tracing::warn!(
                    target: "blackflower_server",
                    event_name = "network_resume_state_missing",
                    "resume token has no retained authoritative session",
                );
                let removed = self.peers.swap_remove(new_index);
                removed.try_despawn(&self.simulation);
            }
        }
    }
}

struct DetachedSession {
    binding: ControlBinding,
    expires_at: Duration,
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
    session_id: Option<SessionId>,
    pending_replacement: Option<SessionId>,
    bootstrap_applied: bool,
    clock_uncertainty_ticks: Option<u16>,
    activation_scheduled: bool,
    active_reported: bool,
    binding: Option<ControlBinding>,
    spawn_pending: bool,
    binding_announced: bool,
    last_snapshot_tick: Option<u64>,
    operational_state: OperationalState,
    last_authenticated_traffic: Duration,
    last_snapshot_applied: Duration,
    last_input: Duration,
}

impl PeerRuntime {
    fn new(peer: NetworkPeer, now: Duration) -> Self {
        Self {
            peer,
            session_id: None,
            pending_replacement: None,
            bootstrap_applied: false,
            clock_uncertainty_ticks: None,
            activation_scheduled: false,
            active_reported: false,
            binding: None,
            spawn_pending: false,
            binding_announced: false,
            last_snapshot_tick: None,
            operational_state: OperationalState::Nominal,
            last_authenticated_traffic: now,
            last_snapshot_applied: now,
            last_input: now,
        }
    }

    fn service(
        &mut self,
        network: &mut DedicatedServerNetwork<LoopbackSessionAuthority>,
        simulation: &SimulationStatus,
        movement_frame: &MovementFrame,
        entities: &mut EntityIdAllocator,
        tick: SimulationTick,
        now: Duration,
    ) -> Result<(), ServerNetworkRuntimeError> {
        self.peer.record_metrics();
        for _index in 0..MAX_EVENTS_PER_PEER {
            let Some(event) = self.peer.poll_event()? else {
                break;
            };
            self.handle_event(network, simulation, entities, event, tick, now)?;
        }
        self.update_operational_state(now)?;
        if self.pending_replacement.is_some() {
            return Ok(());
        }
        self.try_finish_spawn(movement_frame)?;
        self.try_schedule_activation(tick)?;
        if self.activation_scheduled && !self.active_reported && self.peer.advance(tick)? {
            self.active_reported = true;
            self.last_snapshot_applied = now;
            self.last_input = now;
            tracing::info!(
                target: "blackflower_server",
                event_name = "network_peer_active",
                tick = tick.get(),
                "application session activated",
            );
        }
        self.try_queue_snapshot(movement_frame, now)?;
        Ok(())
    }

    fn handle_event(
        &mut self,
        network: &mut DedicatedServerNetwork<LoopbackSessionAuthority>,
        simulation: &SimulationStatus,
        entities: &mut EntityIdAllocator,
        event: NetworkEvent,
        tick: SimulationTick,
        now: Duration,
    ) -> Result<(), ServerNetworkRuntimeError> {
        match event {
            NetworkEvent::SessionControl(frame) => {
                self.handle_session_control_event(network, simulation, entities, &frame, now)
            }
            NetworkEvent::Datagram(datagram) => {
                self.handle_datagram(simulation, &datagram, tick, now)
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

    fn handle_session_control_event(
        &mut self,
        network: &mut DedicatedServerNetwork<LoopbackSessionAuthority>,
        simulation: &SimulationStatus,
        entities: &mut EntityIdAllocator,
        frame: &[u8],
        now: Duration,
    ) -> Result<(), ServerNetworkRuntimeError> {
        let message = decode_control_message(frame)?;
        let snapshot_applied = matches!(&message, SessionControlMessage::BootstrapApplied { .. });
        self.handle_control(network, simulation, entities, message, now)?;
        self.last_authenticated_traffic = now;
        if snapshot_applied {
            self.last_snapshot_applied = now;
        }
        Ok(())
    }

    fn handle_datagram(
        &mut self,
        simulation: &SimulationStatus,
        datagram: &[u8],
        tick: SimulationTick,
        now: Duration,
    ) -> Result<(), ServerNetworkRuntimeError> {
        let header = decode_datagram(datagram)?.header;
        let mut input_received = false;
        let mut snapshot_applied = false;
        match header.flow {
            FlowId::TimeSync => self.peer.respond_time_sync(datagram, now)?,
            FlowId::Input if self.peer.session().state() == SessionState::Active => {
                let ingress = self.peer.ingest_input(datagram, tick, now, true)?;
                input_received = true;
                snapshot_applied = ingress.snapshot_applied;
                self.submit_input(simulation, ingress, tick)?;
            }
            FlowId::Input
            | FlowId::SnapshotDelta
            | FlowId::SnapshotAppliedAck
            | FlowId::VoiceCapture
            | FlowId::VoiceDelivery => {
                return Err(ServerNetworkRuntimeError::UnexpectedDatagramFlow);
            }
        }
        self.last_authenticated_traffic = now;
        if input_received {
            self.last_input = now;
        }
        if snapshot_applied {
            self.last_snapshot_applied = now;
        }
        Ok(())
    }

    fn handle_control(
        &mut self,
        network: &mut DedicatedServerNetwork<LoopbackSessionAuthority>,
        simulation: &SimulationStatus,
        entities: &mut EntityIdAllocator,
        message: SessionControlMessage,
        now: Duration,
    ) -> Result<(), ServerNetworkRuntimeError> {
        match message {
            SessionControlMessage::AdmissionRequest { protocol_revision } => {
                self.negotiate(network, protocol_revision, now)
            }
            SessionControlMessage::ContentReady(content) => {
                network.content_ready(&mut self.peer, &content)?;
                self.request_spawn(simulation, entities)
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
                self.restart_synchronization()
            }
            SessionControlMessage::ResumeRequest { token } => {
                let resumed = network.resume(&mut self.peer, &token, now)?;
                self.session_id = Some(resumed.claims.session_id);
                self.pending_replacement = Some(resumed.invalidate_session);
                self.restart_synchronization()
            }
            SessionControlMessage::AdmissionAccepted { .. }
            | SessionControlMessage::AdmissionRejected(_)
            | SessionControlMessage::ContentManifest(_)
            | SessionControlMessage::ControlBinding(_)
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
        let admitted = match network.admit(&mut self.peer, protocol_revision, now) {
            Ok(admitted) => admitted,
            Err(error) => {
                let reason = admission_rejection(&error);
                self.peer.reject_admission(reason)?;
                return Err(ServerNetworkRuntimeError::Peer(error));
            }
        };
        self.session_id = Some(admitted.claims.session_id);
        Ok(())
    }

    fn restart_synchronization(&mut self) -> Result<(), ServerNetworkRuntimeError> {
        self.bootstrap_applied = false;
        self.clock_uncertainty_ticks = None;
        self.activation_scheduled = false;
        self.active_reported = false;
        self.last_snapshot_tick = None;
        self.spawn_pending = true;
        Ok(())
    }

    fn adopt_resumed_binding(&mut self, binding: ControlBinding) {
        self.binding = Some(binding);
        self.binding_announced = false;
        self.spawn_pending = true;
        self.pending_replacement = None;
    }

    fn request_spawn(
        &mut self,
        simulation: &SimulationStatus,
        entities: &mut EntityIdAllocator,
    ) -> Result<(), ServerNetworkRuntimeError> {
        if self.binding.is_some() {
            return Err(ServerNetworkRuntimeError::DuplicateControlBinding);
        }
        let replicated = entities.allocate()?;
        let controlled_entity = NonZeroU64::new(replicated.get())
            .ok_or(ServerNetworkRuntimeError::InvalidActorIdentity)?;
        let binding = ControlBinding {
            control_epoch: 1,
            controlled_entity,
        };
        simulation.try_spawn_actor(actor_id(binding))?;
        self.binding = Some(binding);
        self.spawn_pending = true;
        Ok(())
    }

    fn try_finish_spawn(
        &mut self,
        movement_frame: &MovementFrame,
    ) -> Result<(), ServerNetworkRuntimeError> {
        if !self.spawn_pending {
            return Ok(());
        }
        let binding = self
            .binding
            .ok_or(ServerNetworkRuntimeError::MissingControlBinding)?;
        let owner = actor_id(binding);
        if movement_frame.actor(owner).is_none() {
            return Ok(());
        }
        if !self.binding_announced {
            self.peer.send_control_binding(binding)?;
            self.binding_announced = true;
        }
        let snapshot = project_movement_frame(movement_frame, owner)?;
        let _bootstrap = self.peer.queue_bootstrap(snapshot)?;
        self.last_snapshot_tick = Some(movement_frame.tick().get());
        self.spawn_pending = false;
        Ok(())
    }

    fn try_queue_snapshot(
        &mut self,
        movement_frame: &MovementFrame,
        now: Duration,
    ) -> Result<(), ServerNetworkRuntimeError> {
        if self.peer.session().state() != SessionState::Active
            || self.spawn_pending
            || !movement_frame
                .tick()
                .get()
                .is_multiple_of(SNAPSHOT_INTERVAL_TICKS)
            || self
                .last_snapshot_tick
                .is_some_and(|last| last >= movement_frame.tick().get())
        {
            return Ok(());
        }
        let binding = self
            .binding
            .ok_or(ServerNetworkRuntimeError::MissingControlBinding)?;
        let snapshot = project_movement_frame(movement_frame, actor_id(binding))?;
        self.peer.queue_snapshot(snapshot, now)?;
        self.last_snapshot_tick = Some(movement_frame.tick().get());
        Ok(())
    }

    fn submit_input(
        &mut self,
        simulation: &SimulationStatus,
        ingress: InputIngress,
        current_tick: SimulationTick,
    ) -> Result<(), ServerNetworkRuntimeError> {
        let binding = self
            .binding
            .ok_or(ServerNetworkRuntimeError::MissingControlBinding)?;
        if ingress.control_epoch != binding.control_epoch
            || ingress.controlled_entity != binding.controlled_entity
        {
            return Err(ServerNetworkRuntimeError::WrongControlBinding);
        }
        let command_codec = NoCommandsCodec;
        for classified in &ingress.commands {
            validate_command_codec(ProtocolRevision::V1, &command_codec, &classified.command)?;
        }
        let control_codec = MovementControlCodec;
        let actor = actor_id(binding);
        let mut controls = Vec::with_capacity(ingress.frames.len());
        for frame in ingress.frames {
            validate_control_codec(ProtocolRevision::V1, &control_codec, &frame)?;
            if frame.execute_tick.get()
                > current_tick.get().saturating_add(MAX_FUTURE_COMMAND_TICKS)
            {
                return Err(ServerNetworkRuntimeError::ControlTooFarInFuture);
            }
            let wire = WireMovementControl::decode(&frame.payload)?;
            let movement = wire.movement();
            controls.push(MovementControl::new(
                actor,
                frame.sequence.get(),
                blackflower_world_simulation::SimulationTick::new(frame.execute_tick.get()),
                movement,
                wire.view_yaw().dequantize(),
                wire.view_pitch().dequantize(),
            )?);
        }
        simulation.try_submit_controls(controls)?;
        Ok(())
    }

    fn update_operational_state(&mut self, now: Duration) -> Result<(), ServerNetworkRuntimeError> {
        let active = self.peer.session().state() == SessionState::Active;
        let state = operational_state(
            now,
            self.last_authenticated_traffic,
            if active {
                self.last_snapshot_applied
            } else {
                now
            },
            if active { self.last_input } else { now },
            false,
        );
        if state != self.operational_state {
            tracing::info!(
                target: "blackflower_server",
                event_name = "network_operational_state_changed",
                state = ?state,
                "network session operational state changed",
            );
            self.operational_state = state;
        }
        if state == OperationalState::ConnectionUnresponsive {
            Err(ServerNetworkRuntimeError::ConnectionUnresponsive)
        } else {
            Ok(())
        }
    }

    fn detach(mut self, now: Duration) -> Option<(SessionId, DetachedSession)> {
        let session_id = self.session_id?;
        let binding = self.binding.take()?;
        Some((
            session_id,
            DetachedSession {
                binding,
                expires_at: now.saturating_add(RECONNECT_WINDOW),
            },
        ))
    }

    fn try_despawn(self, simulation: &SimulationStatus) {
        let Some(binding) = self.binding else {
            return;
        };
        try_despawn_binding(simulation, binding);
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
    /// Canonical snapshot construction failed.
    #[error(transparent)]
    Snapshot(#[from] blackflower_networking_replication::SnapshotError),
    /// Session-control decoding failed.
    #[error(transparent)]
    Wire(#[from] blackflower_networking::WireError),
    /// The stable replicated entity identity domain was exhausted.
    #[error(transparent)]
    Identity(#[from] blackflower_networking_replication::IdentityError),
    /// The bounded simulation handoff rejected ingress or state access.
    #[error(transparent)]
    SimulationIngress(#[from] crate::SimulationIngressError),
    /// Sealed movement state could not be projected into schema v1.
    #[error(transparent)]
    Projection(#[from] crate::SimulationProjectionError),
    /// Canonical v1 control bytes could not be decoded.
    #[error(transparent)]
    Protocol(#[from] blackflower_networking_protocol::v1::ProtocolError),
    /// The transport-independent movement runtime rejected converted control.
    #[error(transparent)]
    Movement(#[from] blackflower_world_simulation::MovementError),
    /// Client clock did not satisfy the activation threshold.
    #[error("client clock uncertainty is {uncertainty_ticks} ticks")]
    ClockNotReady {
        /// Client-observed uncertainty rounded upward to ticks.
        uncertainty_ticks: u16,
    },
    /// A transport stopped normally or because its bounded I/O task failed.
    #[error("peer transport stopped")]
    TransportStopped,
    /// Authenticated application traffic stopped for the normative five seconds.
    #[error("peer application traffic is unresponsive")]
    ConnectionUnresponsive,
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
    /// Content readiness attempted to assign a second controlled actor.
    #[error("session already has a controlled actor")]
    DuplicateControlBinding,
    /// Session lifecycle reached gameplay without an assigned actor.
    #[error("session has no controlled actor")]
    MissingControlBinding,
    /// Replication produced an impossible zero actor identity.
    #[error("replication allocated an invalid actor identity")]
    InvalidActorIdentity,
    /// An input datagram targeted a different actor or control generation.
    #[error("input datagram does not match the session control binding")]
    WrongControlBinding,
    /// A control frame exceeded the bounded future-input window.
    #[error("control frame is too far in the future")]
    ControlTooFarInFuture,
}

const fn retains_peer(error: &ServerNetworkRuntimeError) -> bool {
    matches!(
        error,
        ServerNetworkRuntimeError::Peer(PeerError::BandwidthUnavailable)
    )
}

const fn preserves_session_for_resume(error: &ServerNetworkRuntimeError) -> bool {
    matches!(
        error,
        ServerNetworkRuntimeError::TransportStopped
            | ServerNetworkRuntimeError::ConnectionUnresponsive
    )
}

fn try_despawn_binding(simulation: &SimulationStatus, binding: ControlBinding) {
    if let Err(error) = simulation.try_despawn_actor(actor_id(binding)) {
        tracing::warn!(
            target: "blackflower_server",
            event_name = "simulation_actor_despawn_deferred",
            %error,
            "could not queue actor despawn",
        );
    }
}

fn actor_id(binding: ControlBinding) -> ActorId {
    ActorId::new(binding.controlled_entity)
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
