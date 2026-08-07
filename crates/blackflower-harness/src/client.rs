use std::collections::{BTreeMap, VecDeque};
use std::error::Error as StdError;
use std::time::Duration;

use crate::input::{InputBuildError, InputSender};
use crate::snapshots::{AppliedSnapshot, SnapshotInbox, SnapshotInboxError};
use crate::{
    ClientEvent, ClientHarnessConfig, ClientPrediction, ClientTransport, ClientTransportEvent,
    ClientView, ControlBinding, ControlSubmission, PredictionUpdate, TraceObserver, TraceRecord,
};
use blackflower_networking::{
    BootstrapId, CLOCK_SAMPLE_TIMEOUT, ClockError, ClockFilter, ClockSafety, ContentRejectReason,
    DatagramHeader, FlowId, FlowSequence, INITIAL_TIME_SYNC_SAMPLES, InputAction, ResyncAction,
    ResyncReason, SessionControlMessage, SessionError, SessionState, SimulationTick,
    SnapshotAction, SnapshotAppliedAck, StateBootstrapHeader, TimeSyncMessage, TimeSyncSchedule,
    WireError, decode_control_message, decode_datagram, decode_snapshot_chunk, decode_time_sync,
    encode_control_message, encode_datagram, encode_time_sync, record_bootstrap,
    record_clock_uncertainty, record_inputs, record_resync, record_snapshot, record_voice,
};

const MAX_EVENTS_PER_UPDATE: usize = 128;
const MAX_PENDING_EVENTS: usize = 256;

/// Stateful client-runtime binding shared by human frontends and bot controllers.
pub struct ClientHarness<T, P> {
    transport: T,
    prediction: P,
    session: blackflower_networking::ClientSession,
    input: InputSender,
    snapshots: SnapshotInbox,
    events: VecDeque<ClientEvent>,
    pending_offer: Option<BootstrapOffer>,
    pending_transfer: Option<BootstrapTransfer>,
    clock: ClockFilter,
    time_sync_schedule: Option<TimeSyncSchedule>,
    pending_time_sync: BTreeMap<u32, u64>,
    next_time_sync_exchange: u32,
    observed_time_sync: u8,
    clock_ready_reported: bool,
    installed_content_set_id: blackflower_networking::RequiredContentSetId,
    content: Option<blackflower_networking::ContentManifest>,
    trace: Option<Box<dyn TraceObserver>>,
}

impl<T, P> ClientHarness<T, P>
where
    T: ClientTransport,
    P: ClientPrediction,
{
    /// Start application admission over an already secure QUIC client handle.
    pub fn new(
        mut transport: T,
        prediction: P,
        config: ClientHarnessConfig,
    ) -> Result<Self, ClientHarnessError<T::Error, P::Error>> {
        let initial_epoch = blackflower_networking::ConnectionEpoch::new(0);
        let mut session =
            blackflower_networking::ClientSession::new(config.compatibility, initial_epoch);
        session.secure()?;
        session.negotiate()?;
        let admission = SessionControlMessage::AdmissionRequest {
            protocol_revision: config.compatibility.protocol_revision,
        };
        transport
            .send_control(encode_control_message(&admission)?)
            .map_err(ClientHarnessError::Transport)?;
        Ok(Self {
            transport,
            prediction,
            session,
            input: InputSender::new(initial_epoch),
            snapshots: SnapshotInbox::new(),
            events: VecDeque::new(),
            pending_offer: None,
            pending_transfer: None,
            clock: ClockFilter::new(),
            time_sync_schedule: None,
            pending_time_sync: BTreeMap::new(),
            next_time_sync_exchange: 0,
            observed_time_sync: 0,
            clock_ready_reported: false,
            installed_content_set_id: config.installed_content_set_id,
            content: None,
            trace: None,
        })
    }

    /// Drain bounded transport work and advance scheduled session activation.
    pub fn update(
        &mut self,
        now: Duration,
        authoritative_tick: SimulationTick,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.transport.record_metrics();
        for _index in 0..MAX_EVENTS_PER_UPDATE {
            if self.events.len() >= MAX_PENDING_EVENTS {
                break;
            }
            let event = self
                .transport
                .receive()
                .map_err(ClientHarnessError::Transport)?;
            let Some(event) = event else {
                break;
            };
            self.handle_transport_event(event, now)?;
        }
        self.send_due_time_sync(now)?;
        self.advance_activation(authoritative_tick, now)
    }

    /// Install or replace the server-authorized controlled-object binding.
    pub fn set_control_binding(&mut self, binding: ControlBinding) {
        self.input.set_binding(binding);
    }

    /// Begin a fresh consecutive control-tick run after a local pacing discontinuity.
    ///
    /// Input identities remain monotonic. Only redundant control frames from the
    /// previous run are discarded, allowing the next accepted submission to
    /// choose a new future execution tick.
    pub fn reset_control_timeline(&mut self) {
        self.input.reset_control_timeline();
    }

    /// Install a boundary trace sink that records every accepted submission.
    ///
    /// The sink observes the same information a fair client has (perceived
    /// projection plus submitted input) and nothing more, which is what makes a
    /// captured corpus valid imitation-learning data. Passing a new sink
    /// replaces any previously installed one.
    pub fn set_trace_observer(&mut self, observer: Box<dyn TraceObserver>) {
        self.trace = Some(observer);
    }

    /// Remove and return the installed boundary trace sink, if any.
    pub fn take_trace_observer(&mut self) -> Option<Box<dyn TraceObserver>> {
        self.trace.take()
    }

    /// Accept source-neutral canonical control, queue prediction, and publish input.
    pub fn submit_control(
        &mut self,
        submission: ControlSubmission,
    ) -> Result<blackflower_networking::InputSequence, ClientHarnessError<T::Error, P::Error>> {
        if self.session.state() != SessionState::Active {
            return Err(ClientHarnessError::SessionNotActive);
        }
        // Clone the submission only while a trace sink is attached, so ordinary
        // play pays nothing. The clone captures the accepted input before
        // `build` consumes it, letting the tee fire after the submission is
        // fully committed below.
        let traced = self.trace.as_ref().map(|_sink| submission.clone());
        let mut next_input = self.input.clone();
        let (sequence, frame, datagram) = next_input.build(submission)?;
        self.prediction
            .queue_control(&frame)
            .map_err(ClientHarnessError::Prediction)?;
        self.transport
            .set_latest_input(datagram)
            .map_err(ClientHarnessError::Transport)?;
        self.input = next_input;
        record_inputs(InputAction::Submitted, 1);
        if let Some(submission) = traced.as_ref() {
            self.record_submission(sequence, submission);
        }
        Ok(sequence)
    }

    /// Advance the common prediction timeline to an externally paced target tick.
    pub fn advance_prediction_to(
        &mut self,
        target: SimulationTick,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.prediction
            .advance_to(target)
            .map_err(ClientHarnessError::Prediction)
    }

    /// Map client monotonic time into the current estimated authoritative tick.
    pub fn estimated_server_tick(&mut self, now: Duration) -> Result<SimulationTick, ClockError> {
        let server_micros = self.clock.map_local_micros(duration_micros(now))?;
        Ok(blackflower_networking::server_micros_to_tick(server_micros))
    }

    /// Return the adaptive future lead used to schedule canonical controls.
    #[must_use]
    pub fn input_lead_ticks(&self) -> u64 {
        self.clock.input_lead_ticks()
    }

    /// Ask the server for a bounded full-state resynchronization.
    pub fn request_resync(
        &mut self,
        now: Duration,
        reason: ResyncReason,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.session.begin_resync(now)?;
        self.send_control(SessionControlMessage::ResyncRequest { reason })?;
        record_resync(ResyncAction::Requested);
        Ok(())
    }

    /// Replace a stopped connection and present its fresh one-use resume token.
    pub fn reconnect(
        &mut self,
        transport: T,
        epoch: blackflower_networking::ConnectionEpoch,
        token: Vec<u8>,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.session.reconnect(epoch)?;
        self.transport = transport;
        self.input.reconnect(epoch);
        self.pending_offer = None;
        self.pending_transfer = None;
        self.send_control(SessionControlMessage::ResumeRequest { token })
    }

    /// Return the immutable session, authoritative, and predicted client view.
    #[must_use]
    pub fn view(&self) -> ClientView<'_, P::State> {
        ClientView {
            session_state: self.session.state(),
            authoritative: self.snapshots.window(),
            predicted: self.prediction.predicted_state(),
            content: self.content.as_ref(),
            pending_events: self.events.len(),
        }
    }

    /// Consume the oldest client-facing event.
    pub fn pop_event(&mut self) -> Option<ClientEvent> {
        self.events.pop_front()
    }

    /// Return the shared prediction coordinator for simulation-specific setup.
    #[must_use]
    pub const fn prediction(&self) -> &P {
        &self.prediction
    }

    /// Return the shared prediction coordinator for simulation-specific setup.
    #[must_use]
    pub const fn prediction_mut(&mut self) -> &mut P {
        &mut self.prediction
    }

    fn record_submission(
        &mut self,
        input_sequence: blackflower_networking::InputSequence,
        submission: &ControlSubmission,
    ) {
        let session_state = self.session.state();
        let window = self.snapshots.window();
        let authoritative = window.newest();
        if let Some(observer) = self.trace.as_mut() {
            observer.on_control_submitted(TraceRecord {
                session_state,
                input_sequence,
                authoritative,
                window,
                submission,
            });
        }
    }

    fn handle_transport_event(
        &mut self,
        event: ClientTransportEvent,
        now: Duration,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        match event {
            ClientTransportEvent::SessionControl(frame) => self.handle_control(&frame, now),
            ClientTransportEvent::Datagram(datagram) => self.handle_datagram(datagram, now),
            ClientTransportEvent::Bootstrap { header, body } => {
                self.pending_transfer = Some(BootstrapTransfer { header, body });
                self.try_apply_bootstrap()
            }
            ClientTransportEvent::PathChanged { previous, current } => {
                if let Some(schedule) = self.time_sync_schedule.as_mut() {
                    schedule.path_changed(now);
                }
                self.clock.path_changed();
                self.pending_time_sync.clear();
                self.observed_time_sync = 0;
                self.clock_ready_reported = false;
                self.events
                    .push_back(ClientEvent::PathChanged { previous, current });
                Ok(())
            }
            ClientTransportEvent::TransportStopped => self.transport_stopped(),
        }
    }

    fn handle_control(
        &mut self,
        frame: &[u8],
        now: Duration,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        match decode_control_message(frame)? {
            SessionControlMessage::AdmissionAccepted {
                claims,
                connection_epoch,
            } => self.admitted(&claims, connection_epoch, now),
            SessionControlMessage::AdmissionRejected(reason) => self.admission_rejected(reason),
            SessionControlMessage::ContentManifest(manifest) => self.content_manifest(manifest),
            SessionControlMessage::BootstrapOffer {
                bootstrap_id,
                snapshot_tick,
                digest,
                length,
            } => self.bootstrap_offered(BootstrapOffer {
                bootstrap_id,
                snapshot_tick,
                digest,
                length,
            }),
            SessionControlMessage::ActivateAt { tick } => self.activation_scheduled(tick),
            SessionControlMessage::ControlBinding(binding) => self.control_bound(binding),
            SessionControlMessage::ResumeIssued {
                token,
                expires_in_millis,
            } => self.resume_issued(token, expires_in_millis),
            SessionControlMessage::CommandDisposition {
                command_id,
                disposition,
            } => self.command_disposition(command_id, disposition),
            SessionControlMessage::Closing { code } => self.server_closing(code),
            SessionControlMessage::AdmissionRequest { .. }
            | SessionControlMessage::ContentReady(_)
            | SessionControlMessage::ContentRejected(_)
            | SessionControlMessage::BootstrapApplied { .. }
            | SessionControlMessage::ClockSynchronized { .. }
            | SessionControlMessage::ResyncRequest { .. }
            | SessionControlMessage::ResumeRequest { .. } => {
                Err(ClientHarnessError::UnexpectedControlMessage)
            }
        }
    }

    fn control_bound(
        &mut self,
        binding: blackflower_networking::ControlBinding,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.input.set_binding(binding);
        self.events.push_back(ClientEvent::ControlBound(binding));
        Ok(())
    }

    fn command_disposition(
        &mut self,
        command_id: blackflower_networking::CommandId,
        disposition: blackflower_networking::CommandDisposition,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        if !matches!(
            disposition,
            blackflower_networking::CommandDisposition::Queued { .. }
        ) {
            self.input.acknowledge_command(command_id);
        }
        self.events.push_back(ClientEvent::CommandDisposition {
            command_id,
            disposition,
        });
        Ok(())
    }

    fn resume_issued(
        &mut self,
        token: Vec<u8>,
        expires_in_millis: u32,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.events.push_back(ClientEvent::ResumeIssued {
            token,
            expires_in_millis,
        });
        Ok(())
    }

    fn handle_datagram(
        &mut self,
        datagram: bytes::Bytes,
        now: Duration,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        let decoded = decode_datagram(&datagram)?;
        if decoded.header.connection_epoch != self.session.connection_epoch() {
            return Err(ClientHarnessError::WrongConnectionEpoch);
        }
        match decoded.header.flow {
            FlowId::SnapshotDelta => {
                let chunk = decode_snapshot_chunk(decoded.payload, decoded.payload.len())?;
                self.handle_snapshot_chunk(chunk, now)
            }
            FlowId::TimeSync => self.handle_time_sync(decoded.payload, now),
            FlowId::VoiceDelivery => {
                record_voice(blackflower_networking::MetricDirection::Downstream);
                self.events.push_back(ClientEvent::VoiceDatagram(datagram));
                Ok(())
            }
            FlowId::Input | FlowId::SnapshotAppliedAck | FlowId::VoiceCapture => {
                Err(ClientHarnessError::UnexpectedDatagramFlow)
            }
        }
    }

    fn handle_time_sync(
        &mut self,
        payload: &[u8],
        now: Duration,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        let message = decode_time_sync(payload)?;
        let TimeSyncMessage::Response {
            exchange_id,
            client_send_micros,
            server_receive_micros,
            server_send_micros,
        } = message
        else {
            return Err(ClientHarnessError::UnexpectedTimeSyncMessage);
        };
        let expected = self
            .pending_time_sync
            .remove(&exchange_id)
            .ok_or(ClientHarnessError::UnexpectedTimeSyncResponse)?;
        if expected != client_send_micros {
            return Err(ClientHarnessError::UnexpectedTimeSyncResponse);
        }
        self.clock.observe(
            blackflower_networking::ClockSample {
                client_send_micros,
                server_receive_micros,
                server_send_micros,
                client_receive_micros: duration_micros(now),
            },
            now,
        )?;
        self.observed_time_sync = self.observed_time_sync.saturating_add(1);
        let uncertainty = self.clock.uncertainty_ticks();
        record_clock_uncertainty(uncertainty);
        self.events.push_back(ClientEvent::TimeSync(message));
        if self.observed_time_sync >= INITIAL_TIME_SYNC_SAMPLES
            && self.clock.safety(now) == ClockSafety::ActivationReady
            && !self.clock_ready_reported
        {
            let uncertainty_ticks = u16::try_from(uncertainty)
                .map_err(|_error| ClientHarnessError::ClockUncertaintyOutOfRange)?;
            self.send_control(SessionControlMessage::ClockSynchronized { uncertainty_ticks })?;
            self.clock_ready_reported = true;
        }
        Ok(())
    }

    fn send_due_time_sync(
        &mut self,
        now: Duration,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        let now_micros = duration_micros(now);
        let timeout_micros = duration_micros(CLOCK_SAMPLE_TIMEOUT);
        self.pending_time_sync
            .retain(|_exchange, sent| now_micros.saturating_sub(*sent) < timeout_micros);
        let Some(schedule) = self.time_sync_schedule.as_mut() else {
            return Ok(());
        };
        if !schedule.take_due(now) {
            return Ok(());
        }
        let exchange_id = self.next_time_sync_exchange;
        self.next_time_sync_exchange = exchange_id
            .checked_add(1)
            .ok_or(ClientHarnessError::TimeSyncSequenceExhausted)?;
        let client_send_micros = now_micros;
        let payload = encode_time_sync(TimeSyncMessage::Request {
            exchange_id,
            client_send_micros,
        });
        let datagram = encode_datagram(
            DatagramHeader {
                flow: FlowId::TimeSync,
                connection_epoch: self.session.connection_epoch(),
                flow_sequence: FlowSequence::new(exchange_id),
            },
            &payload,
        );
        self.transport
            .send_time_sync(datagram)
            .map_err(ClientHarnessError::Transport)?;
        self.pending_time_sync
            .insert(exchange_id, client_send_micros);
        Ok(())
    }

    fn handle_snapshot_chunk(
        &mut self,
        chunk: blackflower_networking::SnapshotChunk,
        now: Duration,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        let applied = match self.snapshots.ingest_chunk(chunk, now) {
            Ok(applied) => applied,
            Err(error @ SnapshotInboxError::MissingBaseline { .. }) => {
                return match self.session.state() {
                    SessionState::Active => {
                        self.request_resync(now, ResyncReason::BaselineUnavailable)
                    }
                    SessionState::Resynchronizing => Ok(()),
                    SessionState::Connecting
                    | SessionState::Secure
                    | SessionState::Negotiating
                    | SessionState::ContentChecking
                    | SessionState::Synchronizing
                    | SessionState::Closing => Err(ClientHarnessError::Snapshot(error)),
                };
            }
            Err(error) => return Err(ClientHarnessError::Snapshot(error)),
        };
        let Some(applied) = applied else {
            return Ok(());
        };
        let prediction = self
            .prediction
            .apply_snapshot(&applied.snapshot)
            .map_err(ClientHarnessError::Prediction)?;
        self.finish_snapshot(applied, prediction, now)
    }

    fn finish_snapshot(
        &mut self,
        applied: AppliedSnapshot,
        prediction: PredictionUpdate,
        now: Duration,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.input.set_applied_snapshot(applied.ack);
        record_snapshot(SnapshotAction::Applied);
        self.events.push_back(ClientEvent::SnapshotApplied {
            tick: applied.ack.snapshot_tick,
            prediction: prediction.clone(),
        });
        if matches!(prediction, PredictionUpdate::HardResyncRequired { .. })
            && self.session.state() == SessionState::Active
        {
            self.request_resync(now, ResyncReason::PredictionHistoryMissing)?;
        }
        Ok(())
    }

    fn admitted(
        &mut self,
        claims: &blackflower_networking::AdmissionClaims,
        connection_epoch: blackflower_networking::ConnectionEpoch,
        now: Duration,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.session
            .accept_initial_claims(claims, connection_epoch)?;
        self.input.reconnect(connection_epoch);
        self.time_sync_schedule = Some(TimeSyncSchedule::admission(now));
        Ok(())
    }

    fn content_manifest(
        &mut self,
        manifest: blackflower_networking::ContentManifest,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        if manifest.required_content_set_id != self.installed_content_set_id {
            self.send_control(SessionControlMessage::ContentRejected(
                ContentRejectReason::AssetSetMismatch,
            ))?;
            self.session.close()?;
            self.events.push_back(ClientEvent::ContentRejected {
                required: manifest,
                installed: self.installed_content_set_id,
            });
            return Ok(());
        }
        self.send_control(SessionControlMessage::ContentReady(manifest.clone()))?;
        self.session.synchronize()?;
        self.content = Some(manifest.clone());
        self.events.push_back(ClientEvent::ContentReady(manifest));
        Ok(())
    }

    fn admission_rejected(
        &mut self,
        reason: blackflower_networking::AdmissionRejectReason,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.session.close()?;
        self.events
            .push_back(ClientEvent::AdmissionRejected(reason));
        Ok(())
    }

    fn bootstrap_offered(
        &mut self,
        offer: BootstrapOffer,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        if self.session.state() == SessionState::Resynchronizing {
            self.session.synchronize()?;
        }
        self.pending_offer = Some(offer);
        self.try_apply_bootstrap()
    }

    fn try_apply_bootstrap(&mut self) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        let (Some(offer), Some(transfer)) = (self.pending_offer, self.pending_transfer.as_ref())
        else {
            return Ok(());
        };
        validate_bootstrap_offer(offer, transfer)?;
        let transfer = self
            .pending_transfer
            .take()
            .ok_or(ClientHarnessError::BootstrapMismatch)?;
        self.pending_offer = None;
        let applied = self.snapshots.bootstrap(transfer.header, &transfer.body)?;
        record_bootstrap(transfer.body.len());
        self.input.reset_control_timeline();
        let prediction = self
            .prediction
            .bootstrap(&applied.snapshot)
            .map_err(ClientHarnessError::Prediction)?;
        self.input.set_applied_snapshot(applied.ack);
        record_snapshot(SnapshotAction::Applied);
        self.send_bootstrap_applied(offer, applied.ack)?;
        self.events.push_back(ClientEvent::SnapshotApplied {
            tick: applied.ack.snapshot_tick,
            prediction,
        });
        Ok(())
    }

    fn send_bootstrap_applied(
        &mut self,
        offer: BootstrapOffer,
        ack: SnapshotAppliedAck,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.send_control(SessionControlMessage::BootstrapApplied {
            bootstrap_id: offer.bootstrap_id,
            snapshot_tick: ack.snapshot_tick,
            digest: ack.projection_digest,
        })
    }

    fn activation_scheduled(
        &mut self,
        tick: SimulationTick,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        let current = self
            .snapshots
            .latest()
            .map(|snapshot| SimulationTick::new(snapshot.tick().get()))
            .ok_or(ClientHarnessError::ActivationBeforeBootstrap)?;
        self.session.schedule_activation(current, tick)?;
        Ok(())
    }

    fn advance_activation(
        &mut self,
        current: SimulationTick,
        now: Duration,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        let Some(scheduled) = self.session.scheduled_activation() else {
            return Ok(());
        };
        if current < scheduled || !self.session.advance(current)? {
            return Ok(());
        }
        self.events
            .push_back(ClientEvent::Activated { tick: scheduled });
        if let Some(schedule) = self.time_sync_schedule.as_mut() {
            schedule.set_active(now);
        }
        Ok(())
    }

    fn server_closing(&mut self, code: u16) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.session.close()?;
        self.events.push_back(ClientEvent::Closing { code });
        Ok(())
    }

    fn transport_stopped(&mut self) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        if self.session.state() != SessionState::Closing {
            self.session.close()?;
        }
        self.events.push_back(ClientEvent::TransportStopped);
        Ok(())
    }

    fn send_control(
        &mut self,
        message: SessionControlMessage,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        let frame = encode_control_message(&message)?;
        self.transport
            .send_control(frame)
            .map_err(ClientHarnessError::Transport)
    }
}

#[derive(Debug, Clone, Copy)]
struct BootstrapOffer {
    bootstrap_id: BootstrapId,
    snapshot_tick: SimulationTick,
    digest: blackflower_networking::ProjectionDigest,
    length: u32,
}

struct BootstrapTransfer {
    header: StateBootstrapHeader,
    body: Vec<u8>,
}

/// Failure while coordinating transport, session, replication, and prediction.
#[derive(Debug, thiserror::Error)]
pub enum ClientHarnessError<TE, PE>
where
    TE: StdError + 'static,
    PE: StdError + 'static,
{
    /// Low-level bounded transport operation failed.
    #[error("client transport failed")]
    Transport(#[source] TE),
    /// Simulation-specific prediction coordination failed.
    #[error("client prediction failed")]
    Prediction(#[source] PE),
    /// Application session transition failed.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// Network wire codec failed.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// A time-synchronization sample was internally inconsistent.
    #[error(transparent)]
    Clock(#[from] ClockError),
    /// Snapshot reconstruction or baseline application failed.
    #[error(transparent)]
    Snapshot(#[from] SnapshotInboxError),
    /// Canonical input sequencing or construction failed.
    #[error(transparent)]
    Input(#[from] InputBuildError),
    /// Input was submitted before application-session activation.
    #[error("client session is not active")]
    SessionNotActive,
    /// A server-only control message arrived from the server.
    #[error("server sent a client-originated session-control message")]
    UnexpectedControlMessage,
    /// A client-originated datagram flow arrived from the server.
    #[error("server sent a client-originated datagram flow")]
    UnexpectedDatagramFlow,
    /// Datagram connection generation differs from the active session.
    #[error("datagram belongs to a different connection epoch")]
    WrongConnectionEpoch,
    /// The server sent a time-synchronization request instead of a response.
    #[error("server sent an unexpected time-synchronization message")]
    UnexpectedTimeSyncMessage,
    /// A time-synchronization response does not match an outstanding request.
    #[error("time-synchronization response is not outstanding")]
    UnexpectedTimeSyncResponse,
    /// Time-synchronization exchange identities wrapped.
    #[error("time-synchronization exchange identity exhausted")]
    TimeSyncSequenceExhausted,
    /// Clock uncertainty cannot be represented on the control stream.
    #[error("clock uncertainty exceeds the control-stream representation")]
    ClockUncertaintyOutOfRange,
    /// Activation was scheduled before a full state was applied.
    #[error("session activation was scheduled before bootstrap")]
    ActivationBeforeBootstrap,
    /// Bootstrap reliable-stream and control-stream metadata differ.
    #[error("bootstrap offer does not match the received transfer")]
    BootstrapMismatch,
}

fn duration_micros(value: Duration) -> u64 {
    u64::try_from(value.as_micros()).unwrap_or(u64::MAX)
}

fn validate_bootstrap_offer<TE, PE>(
    offer: BootstrapOffer,
    transfer: &BootstrapTransfer,
) -> Result<(), ClientHarnessError<TE, PE>>
where
    TE: StdError + 'static,
    PE: StdError + 'static,
{
    if offer.bootstrap_id != transfer.header.bootstrap_id
        || offer.snapshot_tick != transfer.header.snapshot_tick
        || offer.digest != transfer.header.projection_digest
        || offer.length != transfer.header.body_length
    {
        Err(ClientHarnessError::BootstrapMismatch)
    } else {
        Ok(())
    }
}
