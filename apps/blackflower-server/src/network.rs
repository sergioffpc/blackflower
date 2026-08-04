use std::sync::{Arc, Mutex};
use std::time::Duration;

use blackflower_acoustics::{VoiceCapturePacket, decode_voice_capture};
use blackflower_networking::{
    AdmissionClaims, AuthorityError, BandwidthScheduler, BootstrapId, BudgetTier,
    CommandDisposition, CommandTimingDecision, CompatibilityContract, ConnectionEpoch,
    ContentManifest, ControlFrame, Deduplication, DeduplicationError, DiscreteCommand, FlowId,
    FlowSequence, InputDeduplicator, MatchEgressBudget, MetricDirection, ProtocolRevision,
    ResumeClaims, ServerSession, SessionAuthority, SessionControlMessage, SessionError,
    SimulationTick, SnapshotAppliedAck, StateBootstrapHeader, TimeSyncMessage, TrafficDirection,
    VoiceBindings, VoiceChannel, VoiceError, VoiceStreamId, WireError, activation_tick,
    classify_command, connection_closed, connection_opened, decode_datagram, decode_input_datagram,
    decode_time_sync, encode_control_message, encode_datagram, encode_snapshot_chunk,
    encode_time_sync, record_bootstrap, record_inputs, record_protocol_violation, record_resync,
    record_snapshot, record_udp_bytes, record_voice,
};
use blackflower_networking::{DatagramHeader, ViolationKind};
use blackflower_networking_quic::{
    BootstrapTransfer, NetworkEvent, QuicError, QuicServer, ServerNetworkHandle,
};
use blackflower_networking_replication::{
    BaselineError, BaselineTracker, DeltaError, Snapshot, SnapshotTick, build_snapshot_chunks,
};

const SNAPSHOT_CHUNK_PAYLOAD_BYTES: usize = 948;

/// Dedicated-server listener with session identity and reconnect authority.
pub struct DedicatedServerNetwork<A> {
    endpoint: QuicServer,
    authority: A,
    contract: CompatibilityContract,
    content: ContentManifest,
    budget_tier: BudgetTier,
    next_connection_epoch: u32,
    match_egress: Arc<Mutex<MatchEgressBudget>>,
}

impl<A: SessionAuthority> DedicatedServerNetwork<A> {
    /// Compose an endpoint, protocol contract, and server-selected map content.
    #[must_use]
    pub fn new(
        endpoint: QuicServer,
        authority: A,
        contract: CompatibilityContract,
        content: ContentManifest,
        budget_tier: BudgetTier,
        now: Duration,
    ) -> Self {
        Self {
            endpoint,
            authority,
            contract,
            content,
            budget_tier,
            next_connection_epoch: 1,
            match_egress: Arc::new(Mutex::new(MatchEgressBudget::new(budget_tier, now))),
        }
    }

    /// Return the bound UDP address of the owned QUIC endpoint.
    pub fn local_addr(&self) -> Result<std::net::SocketAddr, PeerError> {
        Ok(self.endpoint.local_addr()?)
    }

    /// Accept one address-validated connection and start bounded Tokio I/O tasks.
    pub async fn accept(&mut self, now: Duration) -> Result<NetworkPeer, PeerError> {
        let epoch = self.allocate_epoch()?;
        let connection = self.endpoint.accept().await?;
        let handle = connection.spawn_io().await?;
        let mut session = ServerSession::new(self.contract, epoch);
        session.secure()?;
        session.negotiate()?;
        connection_opened();
        Ok(NetworkPeer::new(
            handle,
            session,
            Arc::clone(&self.match_egress),
            self.budget_tier,
            now,
        ))
    }

    /// Negotiate the protocol, assign session identities, and declare map content.
    pub fn admit(
        &mut self,
        peer: &mut NetworkPeer,
        protocol_revision: ProtocolRevision,
        now: Duration,
    ) -> Result<AdmittedSession, PeerError> {
        if protocol_revision != self.contract.protocol_revision {
            return Err(SessionError::Incompatible.into());
        }
        let claims = self.authority.admit(now)?;
        peer.session.accept_claims(&claims)?;
        let resume = self.authority.issue_resume(&claims, now)?;
        peer.send_control(SessionControlMessage::AdmissionAccepted {
            claims,
            connection_epoch: peer.session.connection_epoch(),
        })?;
        peer.send_control(SessionControlMessage::ResumeIssued {
            token: resume.token.clone(),
            expires_in_millis: millis_until(now, resume.expires_at),
        })?;
        peer.send_control(SessionControlMessage::ContentManifest(self.content.clone()))?;
        Ok(AdmittedSession {
            claims,
            resume_token: resume.token,
        })
    }

    /// Accept exact client readiness for the server-owned map requirement.
    pub fn content_ready(
        &self,
        peer: &mut NetworkPeer,
        content: &ContentManifest,
    ) -> Result<(), PeerError> {
        if content != &self.content {
            return Err(PeerError::ContentMismatch);
        }
        peer.session.synchronize()?;
        Ok(())
    }

    /// Consume a one-use reconnect token; caller invalidates the old peer by session ID.
    pub fn resume(
        &mut self,
        peer: &mut NetworkPeer,
        token: &[u8],
        now: Duration,
    ) -> Result<ResumeOutcome, PeerError> {
        blackflower_networking::validate_resume_token(token)?;
        let claims = self.authority.consume_resume(token, now)?;
        peer.session.reconnect(claims.connection_epoch)?;
        Ok(ResumeOutcome {
            invalidate_session: claims.session_id,
            claims,
        })
    }

    fn allocate_epoch(&mut self) -> Result<ConnectionEpoch, PeerError> {
        let value = self.next_connection_epoch;
        self.next_connection_epoch = value.checked_add(1).ok_or(PeerError::EpochExhausted)?;
        Ok(ConnectionEpoch::new(value))
    }
}

/// One protocol-negotiated ordinary session result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedSession {
    /// Exact claims consumed from the external authority.
    pub claims: AdmissionClaims,
    /// Opaque next one-use reconnect token.
    pub resume_token: Vec<u8>,
}

/// Reconnect claims plus the old session identity the host must invalidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeOutcome {
    /// Existing application session whose previous QUIC connection is replaced.
    pub invalidate_session: blackflower_networking::SessionId,
    /// Claims including the fresh connection generation.
    pub claims: ResumeClaims,
}

/// One accepted peer's deterministic protocol, ingress, replication, and queues.
pub struct NetworkPeer {
    handle: ServerNetworkHandle,
    session: ServerSession,
    inputs: InputDeduplicator,
    baselines: BaselineTracker,
    bandwidth: BandwidthScheduler,
    match_egress: Arc<Mutex<MatchEgressBudget>>,
    snapshot_sequence: u32,
    time_sync_sequence: u32,
    bootstrap_sequence: u64,
    pending_bootstrap: Option<(
        BootstrapId,
        SnapshotTick,
        blackflower_networking::ProjectionDigest,
    )>,
    voice_bindings: VoiceBindings,
}

impl NetworkPeer {
    fn new(
        handle: ServerNetworkHandle,
        session: ServerSession,
        match_egress: Arc<Mutex<MatchEgressBudget>>,
        budget_tier: BudgetTier,
        now: Duration,
    ) -> Self {
        Self {
            handle,
            session,
            inputs: InputDeduplicator::default(),
            baselines: BaselineTracker::new(ProtocolRevision::V1),
            bandwidth: BandwidthScheduler::new(budget_tier, now),
            match_egress,
            snapshot_sequence: 0,
            time_sync_sequence: 0,
            bootstrap_sequence: 1,
            pending_bootstrap: None,
            voice_bindings: VoiceBindings::new(),
        }
    }

    /// Return the application session machine.
    #[must_use]
    pub const fn session(&self) -> &ServerSession {
        &self.session
    }

    /// Poll one event emitted by bounded Tokio I/O tasks.
    pub fn poll_event(&self) -> Result<Option<NetworkEvent>, PeerError> {
        Ok(self.handle.try_receive()?)
    }

    /// Reject admission before authoritative player state is created.
    pub fn reject_admission(
        &self,
        reason: blackflower_networking::AdmissionRejectReason,
    ) -> Result<(), PeerError> {
        self.send_control(SessionControlMessage::AdmissionRejected(reason))
    }

    /// Validate one client clock request and queue its four-timestamp response.
    pub fn respond_time_sync(&mut self, datagram: &[u8], now: Duration) -> Result<(), PeerError> {
        let decoded = decode_datagram(datagram)?;
        if decoded.header.flow != FlowId::TimeSync
            || decoded.header.connection_epoch != self.session.connection_epoch()
        {
            record_protocol_violation(ViolationKind::Session);
            return Err(PeerError::WrongTimeSyncFlow);
        }
        let TimeSyncMessage::Request {
            exchange_id,
            client_send_micros,
        } = decode_time_sync(decoded.payload)?
        else {
            return Err(PeerError::UnexpectedTimeSyncMessage);
        };
        let server_receive_micros = duration_micros(now);
        let server_send_micros = duration_micros(now);
        let payload = encode_time_sync(TimeSyncMessage::Response {
            exchange_id,
            client_send_micros,
            server_receive_micros,
            server_send_micros,
        });
        let sequence = self.next_time_sync_sequence()?;
        self.handle.try_send_time_sync(encode_datagram(
            DatagramHeader {
                flow: FlowId::TimeSync,
                connection_epoch: self.session.connection_epoch(),
                flow_sequence: sequence,
            },
            &payload,
        ))?;
        Ok(())
    }

    /// Queue a full uncompressed snapshot before admission, resync, or reconnect activation.
    pub fn queue_bootstrap(&mut self, snapshot: Snapshot) -> Result<BootstrapId, PeerError> {
        if self.session.state() == blackflower_networking::SessionState::Resynchronizing {
            self.session.synchronize()?;
        }
        let (body, digest) = snapshot.encode_with_digest(ProtocolRevision::V1)?;
        let bootstrap_id = BootstrapId::new(self.bootstrap_sequence);
        self.bootstrap_sequence = self
            .bootstrap_sequence
            .checked_add(1)
            .ok_or(PeerError::BootstrapIdExhausted)?;
        let header = StateBootstrapHeader {
            bootstrap_id,
            protocol_revision: ProtocolRevision::V1,
            snapshot_tick: SimulationTick::new(snapshot.tick().get()),
            projection_digest: digest,
            body_length: u32::try_from(body.len())
                .map_err(|_error| WireError::IntegerOutOfRange)?,
        };
        self.handle
            .try_send_bootstrap(BootstrapTransfer { header, body })?;
        self.baselines.record_sent(snapshot)?;
        self.pending_bootstrap = Some((
            bootstrap_id,
            SnapshotTick::new(header.snapshot_tick.get()),
            digest,
        ));
        record_bootstrap(usize::try_from(header.body_length).unwrap_or(usize::MAX));
        self.send_control(SessionControlMessage::BootstrapOffer {
            bootstrap_id,
            snapshot_tick: header.snapshot_tick,
            digest,
            length: header.body_length,
        })?;
        Ok(bootstrap_id)
    }

    /// Validate exact bootstrap application and promote it as the first baseline.
    pub fn bootstrap_applied(
        &mut self,
        bootstrap_id: BootstrapId,
        tick: SimulationTick,
        digest: blackflower_networking::ProjectionDigest,
    ) -> Result<(), PeerError> {
        let expected = self
            .pending_bootstrap
            .ok_or(PeerError::UnexpectedBootstrapAck)?;
        if expected != (bootstrap_id, SnapshotTick::new(tick.get()), digest) {
            return Err(PeerError::UnexpectedBootstrapAck);
        }
        self.baselines.acknowledge(SnapshotAppliedAck {
            snapshot_tick: tick,
            projection_digest: digest,
        })?;
        self.pending_bootstrap = None;
        record_snapshot("applied");
        Ok(())
    }

    /// Schedule a four-tick-aligned activation at least 24 ticks ahead.
    pub fn schedule_activation(
        &mut self,
        current: SimulationTick,
        uncertainty_ticks: u64,
    ) -> Result<SimulationTick, PeerError> {
        let scheduled = activation_tick(current, uncertainty_ticks);
        self.session.schedule_activation(current, scheduled)?;
        self.send_control(SessionControlMessage::ActivateAt { tick: scheduled })?;
        Ok(scheduled)
    }

    /// Advance lifecycle activation at the authoritative simulation tick.
    pub fn advance(&mut self, current: SimulationTick) -> Result<bool, PeerError> {
        Ok(self.session.advance(current)?)
    }

    /// Build, frame, queue, and retain one incremental component snapshot.
    pub fn queue_snapshot(&mut self, snapshot: Snapshot, now: Duration) -> Result<(), PeerError> {
        let delta = self.baselines.build_delta(&snapshot)?;
        let chunks = build_snapshot_chunks(
            &delta,
            &snapshot,
            ProtocolRevision::V1,
            SNAPSHOT_CHUNK_PAYLOAD_BYTES,
        )?;
        let mut datagrams = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let payload = encode_snapshot_chunk(&chunk, SNAPSHOT_CHUNK_PAYLOAD_BYTES)?;
            datagrams.push(encode_datagram(
                DatagramHeader {
                    flow: FlowId::SnapshotDelta,
                    connection_epoch: self.session.connection_epoch(),
                    flow_sequence: self.next_snapshot_sequence()?,
                },
                &payload,
            ));
        }
        let estimated_bytes = datagrams.iter().map(Vec::len).sum();
        let mut match_egress = self
            .match_egress
            .lock()
            .map_err(|_poisoned| PeerError::SharedBudgetUnavailable)?;
        if !self
            .bandwidth
            .reserve_downstream(&mut match_egress, estimated_bytes, now)
        {
            return Err(PeerError::BandwidthUnavailable);
        }
        drop(match_egress);
        self.handle.try_send_snapshot_generation(datagrams)?;
        self.baselines.record_sent_delta(snapshot, delta)?;
        record_snapshot("sent");
        Ok(())
    }

    /// Validate, deduplicate, classify, and deliver one input DATAGRAM.
    pub fn ingest_input(
        &mut self,
        datagram: &[u8],
        now: SimulationTick,
        received_at: Duration,
        clock_safe: bool,
    ) -> Result<InputIngress, PeerError> {
        if !self
            .bandwidth
            .reserve(TrafficDirection::Upstream, datagram.len(), received_at)
        {
            return Err(PeerError::BandwidthUnavailable);
        }
        let decoded = decode_datagram(datagram)?;
        if decoded.header.flow != FlowId::Input
            || decoded.header.connection_epoch != self.session.connection_epoch()
        {
            record_protocol_violation(ViolationKind::Session);
            return Err(PeerError::WrongInputFlow);
        }
        let input = decode_input_datagram(decoded.payload)?;
        if let Some(ack) = input.applied_snapshot {
            self.baselines.acknowledge(ack)?;
            record_snapshot("applied");
        }
        let frames = self.new_frames(&input.frames)?;
        let commands = self.new_commands(&input.commands, now, clock_safe)?;
        record_inputs(frames.len());
        Ok(InputIngress {
            control_epoch: input.control_epoch,
            controlled_entity: input.controlled_entity,
            frames,
            commands,
        })
    }

    /// Bind one authenticated capture stream to its authoritative routing scope.
    pub fn bind_voice_stream(
        &mut self,
        stream: VoiceStreamId,
        channel: VoiceChannel,
    ) -> Result<(), PeerError> {
        self.voice_bindings.bind(stream, channel)?;
        Ok(())
    }

    /// Validate one capture packet and attach only session-owned routing authority.
    pub fn ingest_voice_capture(
        &mut self,
        datagram: &[u8],
        received_at: Duration,
    ) -> Result<AuthenticatedVoiceCapture, PeerError> {
        if !self
            .bandwidth
            .reserve(TrafficDirection::Upstream, datagram.len(), received_at)
        {
            return Err(PeerError::BandwidthUnavailable);
        }
        let decoded = decode_datagram(datagram)?;
        if decoded.header.flow != FlowId::VoiceCapture
            || decoded.header.connection_epoch != self.session.connection_epoch()
        {
            record_protocol_violation(ViolationKind::Session);
            return Err(PeerError::WrongVoiceFlow);
        }
        let packet = decode_voice_capture(decoded.payload)?;
        let channel = self
            .voice_bindings
            .channel(VoiceStreamId(packet.stream.0))?;
        record_voice(MetricDirection::Upstream);
        Ok(AuthenticatedVoiceCapture { packet, channel })
    }

    /// Queue one already routed live voice delivery inside both egress budgets.
    pub fn queue_voice_delivery(
        &mut self,
        stream: VoiceStreamId,
        datagram: Vec<u8>,
        now: Duration,
    ) -> Result<(), PeerError> {
        let mut match_egress = self
            .match_egress
            .lock()
            .map_err(|_poisoned| PeerError::SharedBudgetUnavailable)?;
        if !self
            .bandwidth
            .reserve_downstream(&mut match_egress, datagram.len(), now)
        {
            return Err(PeerError::BandwidthUnavailable);
        }
        drop(match_egress);
        self.handle.try_send_voice(stream, datagram)?;
        record_voice(MetricDirection::Downstream);
        Ok(())
    }

    /// Begin a bounded post-activation full-state resynchronization.
    pub fn begin_resync(&mut self, now: Duration) -> Result<(), PeerError> {
        self.session.begin_resync(now)?;
        record_resync();
        Ok(())
    }

    /// Report a gameplay-owned final disposition without executing the command here.
    pub fn report_command_disposition(
        &self,
        command_id: blackflower_networking::CommandId,
        disposition: CommandDisposition,
    ) -> Result<(), PeerError> {
        self.send_control(SessionControlMessage::CommandDisposition {
            command_id,
            disposition,
        })
    }

    /// Reconcile application estimates with actual UDP bytes reported by Quinn.
    pub fn reconcile_egress(
        &mut self,
        estimated_bytes: usize,
        previous_udp_bytes: u64,
    ) -> Result<(), PeerError> {
        let actual = self
            .handle
            .udp_bytes()
            .transmitted
            .saturating_sub(previous_udp_bytes);
        let actual = usize::try_from(actual).unwrap_or(usize::MAX);
        self.bandwidth
            .reconcile_udp_bytes(TrafficDirection::Downstream, estimated_bytes, actual);
        self.match_egress
            .lock()
            .map_err(|_poisoned| PeerError::SharedBudgetUnavailable)?
            .reconcile_udp_bytes(estimated_bytes, actual);
        record_udp_bytes(MetricDirection::Downstream, actual);
        Ok(())
    }

    /// Reconcile an ingress estimate with the actual UDP receive cost from Quinn.
    pub fn reconcile_ingress(&mut self, estimated_bytes: usize, previous_udp_bytes: u64) {
        let actual = self
            .handle
            .udp_bytes()
            .received
            .saturating_sub(previous_udp_bytes);
        let actual = usize::try_from(actual).unwrap_or(usize::MAX);
        self.bandwidth
            .reconcile_udp_bytes(TrafficDirection::Upstream, estimated_bytes, actual);
        record_udp_bytes(MetricDirection::Upstream, actual);
    }

    fn send_control(&self, message: SessionControlMessage) -> Result<(), PeerError> {
        self.handle
            .try_send_control(encode_control_message(&message)?)?;
        Ok(())
    }

    fn new_frames(&mut self, frames: &[ControlFrame]) -> Result<Vec<ControlFrame>, PeerError> {
        let mut accepted = Vec::new();
        for frame in frames {
            if self.inputs.observe_control(frame)? == Deduplication::New {
                accepted.push(frame.clone());
            }
        }
        Ok(accepted)
    }

    fn new_commands(
        &mut self,
        commands: &[DiscreteCommand],
        now: SimulationTick,
        clock_safe: bool,
    ) -> Result<Vec<ClassifiedCommand>, PeerError> {
        let mut accepted = Vec::new();
        for command in commands {
            if self.inputs.observe_command(command)? == Deduplication::New {
                let decision = classify_command(now, command, clock_safe);
                self.send_command_disposition(command, decision)?;
                accepted.push(ClassifiedCommand {
                    command: command.clone(),
                    decision,
                });
            }
        }
        Ok(accepted)
    }

    fn send_command_disposition(
        &self,
        command: &DiscreteCommand,
        decision: CommandTimingDecision,
    ) -> Result<(), PeerError> {
        let disposition = match decision {
            CommandTimingDecision::Deliver { effective_tick, .. } => {
                CommandDisposition::Queued { effective_tick }
            }
            CommandTimingDecision::Reject(reason) => CommandDisposition::Rejected {
                reason: rejection_code(reason),
            },
        };
        self.send_control(SessionControlMessage::CommandDisposition {
            command_id: command.command_id,
            disposition,
        })
    }

    fn next_snapshot_sequence(&mut self) -> Result<FlowSequence, PeerError> {
        let value = self.snapshot_sequence;
        self.snapshot_sequence = value
            .checked_add(1)
            .ok_or(PeerError::FlowSequenceExhausted)?;
        Ok(FlowSequence::new(value))
    }

    fn next_time_sync_sequence(&mut self) -> Result<FlowSequence, PeerError> {
        let value = self.time_sync_sequence;
        self.time_sync_sequence = value
            .checked_add(1)
            .ok_or(PeerError::FlowSequenceExhausted)?;
        Ok(FlowSequence::new(value))
    }
}

impl Drop for NetworkPeer {
    fn drop(&mut self) {
        connection_closed();
    }
}

/// New canonical input and network-classified commands for the simulation host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputIngress {
    /// Controlled-object generation.
    pub control_epoch: u32,
    /// Non-zero controlled replicated identity.
    pub controlled_entity: std::num::NonZeroU64,
    /// New exact canonical frames; redundant duplicates are omitted.
    pub frames: Vec<ControlFrame>,
    /// New commands with no gameplay execution performed by networking.
    pub commands: Vec<ClassifiedCommand>,
}

/// One new command and its network timing classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedCommand {
    /// Opaque gameplay command bytes and registered kind.
    pub command: DiscreteCommand,
    /// Deliver or reject result from the normative timing policy.
    pub decision: CommandTimingDecision,
}

/// One structurally valid capture packet with session-owned routing scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedVoiceCapture {
    /// Original exact Opus capture packet.
    pub packet: VoiceCapturePacket,
    /// Authoritative proximity, squad, or team binding.
    pub channel: VoiceChannel,
}

/// Dedicated server networking integration failure.
#[derive(Debug, thiserror::Error)]
pub enum PeerError {
    /// QUIC endpoint, stream, datagram, or bounded queue failed.
    #[error(transparent)]
    Quic(#[from] QuicError),
    /// Session lifecycle or compatibility failed.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// Session identity or reconnect authority failed.
    #[error(transparent)]
    Authority(#[from] AuthorityError),
    /// Application wire codec failed.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// Exact-byte input identity was reused inconsistently.
    #[error(transparent)]
    Deduplication(#[from] DeduplicationError),
    /// Applied baseline progression failed.
    #[error(transparent)]
    Baseline(#[from] BaselineError),
    /// Component delta or four-chunk construction failed.
    #[error(transparent)]
    Delta(#[from] DeltaError),
    /// Canonical snapshot codec failed.
    #[error(transparent)]
    Snapshot(#[from] blackflower_networking_replication::SnapshotError),
    /// Existing BFAD v1 voice envelope validation failed.
    #[error(transparent)]
    VoiceDatagram(#[from] blackflower_acoustics::DatagramError),
    /// Session-owned voice binding or routing failed.
    #[error(transparent)]
    Voice(#[from] VoiceError),
    /// Input flow or connection generation does not match this peer.
    #[error("input datagram belongs to the wrong flow or connection epoch")]
    WrongInputFlow,
    /// Voice flow or connection generation does not match this peer.
    #[error("voice datagram belongs to the wrong flow or connection epoch")]
    WrongVoiceFlow,
    /// Time-sync flow or connection generation does not match this peer.
    #[error("time-sync datagram belongs to the wrong flow or connection epoch")]
    WrongTimeSyncFlow,
    /// A client sent a time-sync response instead of a request.
    #[error("client sent an unexpected time-synchronization message")]
    UnexpectedTimeSyncMessage,
    /// Bootstrap application ACK does not match the active transfer exactly.
    #[error("bootstrap applied acknowledgement does not match the active transfer")]
    UnexpectedBootstrapAck,
    /// Client readiness does not echo the server-selected map requirement.
    #[error("client content readiness does not match the server manifest")]
    ContentMismatch,
    /// Connection generation domain was exhausted.
    #[error("connection epoch domain exhausted")]
    EpochExhausted,
    /// Snapshot flow sequence domain was exhausted.
    #[error("snapshot flow sequence domain exhausted")]
    FlowSequenceExhausted,
    /// Bootstrap identity domain was exhausted.
    #[error("bootstrap identity domain exhausted")]
    BootstrapIdExhausted,
    /// The current per-peer or aggregate match egress budget is exhausted.
    #[error("network egress budget is exhausted")]
    BandwidthUnavailable,
    /// The shared aggregate match egress budget is unavailable.
    #[error("shared match egress budget is unavailable")]
    SharedBudgetUnavailable,
}

fn rejection_code(reason: blackflower_networking::CommandRejection) -> u16 {
    match reason {
        blackflower_networking::CommandRejection::TooFarInFuture => 1,
        blackflower_networking::CommandRejection::TooLate => 2,
        blackflower_networking::CommandRejection::InvalidHistoricalTick => 3,
        blackflower_networking::CommandRejection::ClockUnsafe => 4,
    }
}

fn millis_until(now: Duration, expiry: Duration) -> u32 {
    let millis = expiry.saturating_sub(now).as_millis();
    u32::try_from(millis).unwrap_or(u32::MAX)
}

fn duration_micros(value: Duration) -> u64 {
    u64::try_from(value.as_micros()).unwrap_or(u64::MAX)
}
