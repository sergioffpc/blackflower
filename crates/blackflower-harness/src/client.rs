use std::collections::VecDeque;
use std::error::Error as StdError;
use std::time::Duration;

use crate::input::{InputBuildError, InputSender};
use crate::snapshots::{AppliedSnapshot, SnapshotInbox, SnapshotInboxError};
use crate::{
    ClientEvent, ClientHarnessConfig, ClientPrediction, ClientTransport, ClientTransportEvent,
    ClientView, ControlBinding, ControlSubmission, PredictionUpdate,
};
use blackflower_networking::{
    BootstrapId, FlowId, ResyncReason, SessionControlMessage, SessionError, SessionState,
    SimulationTick, SnapshotAppliedAck, StateBootstrapHeader, WireError, decode_control_message,
    decode_datagram, decode_snapshot_chunk, decode_time_sync, encode_control_message,
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
        let mut session = blackflower_networking::ClientSession::new(
            config.compatibility,
            config.connection_epoch,
        );
        session.secure()?;
        session.authenticate()?;
        let admission = SessionControlMessage::AdmissionRequest {
            ticket: config.admission_ticket,
        };
        transport
            .send_control(encode_control_message(&admission)?)
            .map_err(ClientHarnessError::Transport)?;
        Ok(Self {
            transport,
            prediction,
            session,
            input: InputSender::new(config.connection_epoch),
            snapshots: SnapshotInbox::new(),
            events: VecDeque::new(),
            pending_offer: None,
            pending_transfer: None,
        })
    }

    /// Drain bounded transport work and advance scheduled session activation.
    pub fn update(
        &mut self,
        now: Duration,
        authoritative_tick: SimulationTick,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
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
        self.advance_activation(authoritative_tick)
    }

    /// Install or replace the server-authorized controlled-object binding.
    pub fn set_control_binding(&mut self, binding: ControlBinding) {
        self.input.set_binding(binding);
    }

    /// Accept source-neutral canonical control, queue prediction, and publish input.
    pub fn submit_control(
        &mut self,
        submission: ControlSubmission,
    ) -> Result<blackflower_networking::InputSequence, ClientHarnessError<T::Error, P::Error>> {
        if self.session.state() != SessionState::Active {
            return Err(ClientHarnessError::SessionNotActive);
        }
        let mut next_input = self.input.clone();
        let (sequence, frame, datagram) = next_input.build(submission)?;
        self.prediction
            .queue_control(&frame)
            .map_err(ClientHarnessError::Prediction)?;
        self.transport
            .set_latest_input(datagram)
            .map_err(ClientHarnessError::Transport)?;
        self.input = next_input;
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

    /// Ask the server for a bounded full-state resynchronization.
    pub fn request_resync(
        &mut self,
        now: Duration,
        reason: ResyncReason,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.session.begin_resync(now)?;
        self.send_control(SessionControlMessage::ResyncRequest { reason })
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

    fn handle_transport_event(
        &mut self,
        event: ClientTransportEvent,
        now: Duration,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        match event {
            ClientTransportEvent::SessionControl(frame) => self.handle_control(&frame),
            ClientTransportEvent::Datagram(datagram) => self.handle_datagram(&datagram, now),
            ClientTransportEvent::Bootstrap { header, body } => {
                self.pending_transfer = Some(BootstrapTransfer { header, body });
                self.try_apply_bootstrap()
            }
            ClientTransportEvent::PathChanged { previous, current } => {
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
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        match decode_control_message(frame)? {
            SessionControlMessage::AdmissionAccepted(claims) => self.admitted(&claims),
            SessionControlMessage::AdmissionRejected(reason) => self.admission_rejected(reason),
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
            SessionControlMessage::ResumeIssued {
                token,
                expires_in_millis,
            } => {
                self.events.push_back(ClientEvent::ResumeIssued {
                    token,
                    expires_in_millis,
                });
                Ok(())
            }
            SessionControlMessage::CommandDisposition {
                command_id,
                disposition,
            } => {
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
            SessionControlMessage::Closing { code } => self.server_closing(code),
            SessionControlMessage::AdmissionRequest { .. }
            | SessionControlMessage::BootstrapApplied { .. }
            | SessionControlMessage::ResyncRequest { .. }
            | SessionControlMessage::ResumeRequest { .. } => {
                Err(ClientHarnessError::UnexpectedControlMessage)
            }
        }
    }

    fn handle_datagram(
        &mut self,
        datagram: &[u8],
        now: Duration,
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        let decoded = decode_datagram(datagram)?;
        if decoded.header.connection_epoch != self.session.connection_epoch() {
            return Err(ClientHarnessError::WrongConnectionEpoch);
        }
        match decoded.header.flow {
            FlowId::SnapshotDelta => {
                let chunk = decode_snapshot_chunk(decoded.payload, decoded.payload.len())?;
                self.handle_snapshot_chunk(chunk, now)
            }
            FlowId::TimeSync => {
                self.events
                    .push_back(ClientEvent::TimeSync(decode_time_sync(decoded.payload)?));
                Ok(())
            }
            FlowId::VoiceDelivery => {
                self.events
                    .push_back(ClientEvent::VoiceDatagram(datagram.to_vec()));
                Ok(())
            }
            FlowId::Input | FlowId::SnapshotAppliedAck | FlowId::VoiceCapture => {
                Err(ClientHarnessError::UnexpectedDatagramFlow)
            }
        }
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
                    | SessionState::Authenticating
                    | SessionState::Compatible
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
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        self.session.accept_claims(claims)?;
        self.session.synchronize()?;
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
        self.input.reset_control_timeline();
        let prediction = self
            .prediction
            .bootstrap(&applied.snapshot)
            .map_err(ClientHarnessError::Prediction)?;
        self.input.set_applied_snapshot(applied.ack);
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
    ) -> Result<(), ClientHarnessError<T::Error, P::Error>> {
        let Some(scheduled) = self.session.scheduled_activation() else {
            return Ok(());
        };
        if current < scheduled || !self.session.advance(current)? {
            return Ok(());
        }
        self.events
            .push_back(ClientEvent::Activated { tick: scheduled });
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
    /// Activation was scheduled before a full state was applied.
    #[error("session activation was scheduled before bootstrap")]
    ActivationBeforeBootstrap,
    /// Bootstrap reliable-stream and control-stream metadata differ.
    #[error("bootstrap offer does not match the received transfer")]
    BootstrapMismatch,
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
