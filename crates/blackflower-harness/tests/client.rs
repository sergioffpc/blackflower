use std::collections::VecDeque;
use std::error::Error as StdError;
use std::io;
use std::num::NonZeroU64;
use std::str::FromStr as _;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use blackflower_harness::{
    ClientEvent, ClientHarness, ClientHarnessConfig, ClientPrediction, ClientTransport,
    ClientTransportEvent, CommandSubmission, ControlBinding, ControlSubmission, PredictionCodec,
    PredictionDriver, PredictionPass, PredictionSession, PredictionStateComparison,
    PredictionUpdate,
};
use blackflower_networking::{
    AdmissionClaims, BootstrapId, CommandDisposition, CommandTimingClass, CompatibilityContract,
    ConnectionEpoch, ContentManifest, ContentRejectReason, DatagramHeader, FlowId, FlowSequence,
    MapId, MatchId, PlayerId, ProtocolRevision, RequiredContentSetId, SessionControlMessage,
    SessionId, SimulationTick, StateBootstrapHeader, decode_control_message, decode_datagram,
    decode_input_datagram, encode_control_message, encode_datagram, encode_snapshot_chunk,
};
use blackflower_networking_replication::{
    ComponentId, ComponentSampleTick, ComponentState, EntityState, ReplicatedEntityId,
    ReplicationPriority, Snapshot, SnapshotBuilder, SnapshotDelta, SnapshotTick,
    build_snapshot_chunks,
};
use blackflower_world_prediction::{AuthoritativeSnapshot, InputFrame, PredictionTick};

type TestResult = Result<(), Box<dyn StdError>>;

#[test]
fn human_and_bot_controls_use_identical_session_input_contract() -> TestResult {
    let (mut human, human_io) = activated_harness()?;
    let (mut bot, bot_io) = activated_harness()?;
    let binding = ControlBinding {
        control_epoch: 7,
        controlled_entity: NonZeroU64::MIN,
    };
    human.set_control_binding(binding);
    bot.set_control_binding(binding);

    let control = ControlSubmission {
        execute_tick: SimulationTick::new(128),
        payload: vec![3, 1, 4],
        commands: Vec::new(),
    };
    assert_eq!(human.submit_control(control.clone())?.get(), 1);
    assert_eq!(bot.submit_control(control)?.get(), 1);

    let human_datagram = human_io.latest_input()?;
    let bot_datagram = bot_io.latest_input()?;
    assert_eq!(human_datagram, bot_datagram);
    let decoded = decode_datagram(&human_datagram)?;
    let input = decode_input_datagram(decoded.payload)?;
    assert_eq!(input.frames[0].payload, [3, 1, 4]);
    assert_eq!(input.control_epoch, 7);
    Ok(())
}

#[test]
fn prediction_session_reconciles_and_replays_recorded_controls() -> TestResult {
    let driver = CounterDriver::default();
    let codec = CounterCodec;
    let mut prediction = PredictionSession::new(driver, codec);
    let bootstrap = counter_snapshot(0, 0)?;
    assert_eq!(
        prediction.bootstrap(&bootstrap)?,
        PredictionUpdate::Bootstrapped {
            tick: SimulationTick::new(0)
        }
    );

    prediction.queue_control(&blackflower_networking::ControlFrame {
        sequence: blackflower_networking::InputSequence::new(1),
        execute_tick: SimulationTick::new(1),
        payload: vec![1],
    })?;
    prediction.advance_to(SimulationTick::new(4))?;
    assert_eq!(prediction.predicted_state(), Some(&4));

    let update = prediction.apply_snapshot(&counter_snapshot(2, 10)?)?;
    assert_eq!(
        update,
        PredictionUpdate::Reconciled {
            authoritative_tick: SimulationTick::new(2),
            resimulated_ticks: 2,
        }
    );
    assert_eq!(prediction.predicted_state(), Some(&12));
    assert_eq!(
        prediction.driver().passes,
        [
            PredictionPass::Forward,
            PredictionPass::Forward,
            PredictionPass::Forward,
            PredictionPass::Forward,
            PredictionPass::Resimulation,
            PredictionPass::Resimulation,
        ]
    );
    Ok(())
}

#[test]
fn incremental_snapshot_is_applied_and_acknowledged_on_input() -> TestResult {
    let (mut harness, transport) = activated_harness()?;
    let baseline = counter_snapshot(100, 5)?;
    let current = counter_snapshot(108, 7)?;
    let delta = SnapshotDelta::between(&current, Some(&baseline))?;
    let chunks = build_snapshot_chunks(&delta, &current, ProtocolRevision::V1, 1_000)?;
    assert_eq!(chunks.len(), 1);
    let payload = encode_snapshot_chunk(&chunks[0], 1_000)?;
    transport.push(ClientTransportEvent::Datagram(
        encode_datagram(
            DatagramHeader {
                flow: FlowId::SnapshotDelta,
                connection_epoch: ConnectionEpoch::new(1),
                flow_sequence: FlowSequence::new(1),
            },
            &payload,
        )
        .into(),
    ))?;

    harness.update(Duration::from_millis(200), SimulationTick::new(108))?;
    assert_eq!(
        harness
            .view()
            .authoritative()
            .map(blackflower_networking_replication::Snapshot::tick),
        Some(SnapshotTick::new(108))
    );
    let view = harness.view();
    let window = view.authoritative_window();
    assert_eq!(window.len(), 2);
    assert_eq!(
        window
            .iter()
            .map(blackflower_networking_replication::Snapshot::tick)
            .collect::<Vec<_>>(),
        [SnapshotTick::new(100), SnapshotTick::new(108)]
    );
    harness.set_control_binding(default_control_binding());
    harness.submit_control(ControlSubmission {
        execute_tick: SimulationTick::new(128),
        payload: vec![1],
        commands: Vec::new(),
    })?;
    let datagram = transport.latest_input()?;
    let input = decode_input_datagram(decode_datagram(&datagram)?.payload)?;
    assert_eq!(
        input.applied_snapshot.map(|ack| ack.snapshot_tick),
        Some(SimulationTick::new(108))
    );
    Ok(())
}

#[test]
fn queued_commands_remain_redundant_until_a_terminal_disposition() -> TestResult {
    let (mut harness, transport) = activated_harness()?;
    harness.set_control_binding(default_control_binding());
    harness.submit_control(ControlSubmission {
        execute_tick: SimulationTick::new(128),
        payload: vec![1],
        commands: vec![CommandSubmission {
            execute_tick: SimulationTick::new(128),
            view_tick: None,
            timing_class: CommandTimingClass::Interaction,
            kind: 7,
            payload: vec![2],
        }],
    })?;
    let command_id = decode_input_datagram(decode_datagram(&transport.latest_input()?)?.payload)?
        .commands[0]
        .command_id;

    push_command_disposition(
        &transport,
        command_id,
        CommandDisposition::Queued {
            effective_tick: SimulationTick::new(128),
        },
    )?;
    harness.update(Duration::from_millis(300), SimulationTick::new(128))?;
    harness.submit_control(ControlSubmission {
        execute_tick: SimulationTick::new(132),
        payload: vec![1],
        commands: Vec::new(),
    })?;
    let input = decode_input_datagram(decode_datagram(&transport.latest_input()?)?.payload)?;
    assert_eq!(input.commands.len(), 1);
    assert_eq!(input.commands[0].command_id, command_id);

    push_command_disposition(
        &transport,
        command_id,
        CommandDisposition::Committed {
            effective_tick: SimulationTick::new(128),
        },
    )?;
    harness.update(Duration::from_millis(400), SimulationTick::new(132))?;
    harness.submit_control(ControlSubmission {
        execute_tick: SimulationTick::new(136),
        payload: vec![1],
        commands: Vec::new(),
    })?;
    let input = decode_input_datagram(decode_datagram(&transport.latest_input()?)?.payload)?;
    assert!(input.commands.is_empty());
    Ok(())
}

#[test]
fn rejected_prediction_does_not_consume_an_input_identity() -> TestResult {
    let (mut harness, transport) = activated_harness()?;
    harness.set_control_binding(default_control_binding());
    harness.prediction_mut().reject_next_control = true;
    let control = ControlSubmission {
        execute_tick: SimulationTick::new(128),
        payload: vec![1],
        commands: Vec::new(),
    };

    assert!(harness.submit_control(control.clone()).is_err());
    assert!(transport.latest_input().is_err());
    assert_eq!(harness.submit_control(control)?.get(), 1);
    let input = decode_input_datagram(decode_datagram(&transport.latest_input()?)?.payload)?;
    assert_eq!(input.frames[0].sequence.get(), 1);
    Ok(())
}

#[test]
fn changing_control_binding_starts_a_fresh_redundancy_timeline() -> TestResult {
    let (mut harness, transport) = activated_harness()?;
    harness.set_control_binding(default_control_binding());
    harness.submit_control(ControlSubmission {
        execute_tick: SimulationTick::new(128),
        payload: vec![1],
        commands: vec![CommandSubmission {
            execute_tick: SimulationTick::new(128),
            view_tick: None,
            timing_class: CommandTimingClass::Interaction,
            kind: 7,
            payload: vec![2],
        }],
    })?;

    harness.set_control_binding(ControlBinding {
        control_epoch: 2,
        controlled_entity: NonZeroU64::new(2).ok_or("missing entity identity")?,
    });
    harness.submit_control(ControlSubmission {
        execute_tick: SimulationTick::new(200),
        payload: vec![3],
        commands: Vec::new(),
    })?;
    let input = decode_input_datagram(decode_datagram(&transport.latest_input()?)?.payload)?;
    assert_eq!(input.frames.len(), 1);
    assert_eq!(input.frames[0].execute_tick, SimulationTick::new(200));
    assert!(input.commands.is_empty());
    Ok(())
}

#[test]
fn incompatible_server_map_content_is_rejected_before_bootstrap() -> TestResult {
    let transport = FakeTransport::default();
    let control = transport.clone();
    let contract = compatibility();
    let mut harness = ClientHarness::new(
        transport,
        FakePrediction::default(),
        ClientHarnessConfig {
            compatibility: contract,
            installed_content_set_id: RequiredContentSetId::from_bytes([7; 32]),
        },
    )?;
    accept_admission(&control, contract)?;

    harness.update(Duration::ZERO, SimulationTick::new(0))?;

    assert_eq!(
        decode_control_message(&control.sent_control(1)?)?,
        SessionControlMessage::ContentRejected(ContentRejectReason::AssetSetMismatch)
    );
    assert_eq!(
        harness.view().session_state(),
        blackflower_networking::SessionState::Closing
    );
    assert!(matches!(
        harness.pop_event(),
        Some(ClientEvent::ContentRejected { .. })
    ));
    Ok(())
}

type TestHarness = ClientHarness<FakeTransport, FakePrediction>;

fn activated_harness() -> Result<(TestHarness, FakeTransport), Box<dyn StdError>> {
    let transport = FakeTransport::default();
    let control = transport.clone();
    let contract = compatibility();
    let mut harness = ClientHarness::new(
        transport,
        FakePrediction::default(),
        ClientHarnessConfig {
            compatibility: contract,
            installed_content_set_id: content()?.required_content_set_id,
        },
    )?;
    assert!(matches!(
        decode_control_message(&control.sent_control(0)?)?,
        SessionControlMessage::AdmissionRequest { .. }
    ));

    accept_admission(&control, contract)?;
    let snapshot = counter_snapshot(100, 5)?;
    let body = snapshot.encode()?;
    let digest = snapshot.digest(ProtocolRevision::V1)?;
    let header = StateBootstrapHeader {
        bootstrap_id: BootstrapId::new(1),
        protocol_revision: ProtocolRevision::V1,
        snapshot_tick: SimulationTick::new(100),
        projection_digest: digest,
        body_length: u32::try_from(body.len())?,
    };
    control.push(ClientTransportEvent::Bootstrap { header, body })?;
    control.push(ClientTransportEvent::SessionControl(
        encode_control_message(&SessionControlMessage::BootstrapOffer {
            bootstrap_id: header.bootstrap_id,
            snapshot_tick: header.snapshot_tick,
            digest: header.projection_digest,
            length: header.body_length,
        })?,
    ))?;
    control.push(ClientTransportEvent::SessionControl(
        encode_control_message(&SessionControlMessage::ActivateAt {
            tick: SimulationTick::new(124),
        })?,
    ))?;
    harness.update(Duration::ZERO, SimulationTick::new(100))?;
    assert_content_ready(&control)?;
    harness.update(Duration::from_millis(100), SimulationTick::new(124))?;
    assert_eq!(
        harness.view().session_state(),
        blackflower_networking::SessionState::Active
    );
    Ok((harness, control))
}

fn assert_content_ready(transport: &FakeTransport) -> TestResult {
    assert_eq!(
        decode_control_message(&transport.sent_control(1)?)?,
        SessionControlMessage::ContentReady(content()?)
    );
    Ok(())
}

fn accept_admission(transport: &FakeTransport, contract: CompatibilityContract) -> TestResult {
    transport.push(ClientTransportEvent::SessionControl(
        encode_control_message(&SessionControlMessage::AdmissionAccepted {
            claims: claims(contract),
            connection_epoch: ConnectionEpoch::new(1),
        })?,
    ))?;
    transport.push(ClientTransportEvent::SessionControl(
        encode_control_message(&SessionControlMessage::ContentManifest(content()?))?,
    ))?;
    Ok(())
}

fn push_command_disposition(
    transport: &FakeTransport,
    command_id: blackflower_networking::CommandId,
    disposition: CommandDisposition,
) -> TestResult {
    transport.push(ClientTransportEvent::SessionControl(
        encode_control_message(&SessionControlMessage::CommandDisposition {
            command_id,
            disposition,
        })?,
    ))?;
    Ok(())
}

fn default_control_binding() -> ControlBinding {
    ControlBinding {
        control_epoch: 1,
        controlled_entity: NonZeroU64::MIN,
    }
}

fn compatibility() -> CompatibilityContract {
    CompatibilityContract {
        protocol_revision: ProtocolRevision::V1,
    }
}

fn content() -> Result<ContentManifest, Box<dyn StdError>> {
    Ok(ContentManifest {
        map_id: MapId::from_str("maps/test")?,
        required_content_set_id: RequiredContentSetId::from_bytes([2; 32]),
    })
}

fn claims(contract: CompatibilityContract) -> AdmissionClaims {
    AdmissionClaims {
        session_id: SessionId::from_bytes([3; 16]),
        player_id: PlayerId::from_bytes([4; 16]),
        match_id: MatchId::from_bytes([5; 16]),
        protocol_revision: contract.protocol_revision,
    }
}

#[derive(Debug, Default, Clone)]
struct FakeTransport {
    state: Arc<Mutex<FakeTransportState>>,
}

#[derive(Debug, Default)]
struct FakeTransportState {
    incoming: VecDeque<ClientTransportEvent>,
    controls: Vec<Vec<u8>>,
    latest_input: Option<Vec<u8>>,
}

impl FakeTransport {
    fn push(&self, event: ClientTransportEvent) -> io::Result<()> {
        self.state
            .lock()
            .map_err(|_error| io::Error::other("fake transport lock poisoned"))?
            .incoming
            .push_back(event);
        Ok(())
    }

    fn sent_control(&self, index: usize) -> io::Result<Vec<u8>> {
        self.state
            .lock()
            .map_err(|_error| io::Error::other("fake transport lock poisoned"))?
            .controls
            .get(index)
            .cloned()
            .ok_or_else(|| io::Error::other("missing sent control"))
    }

    fn latest_input(&self) -> io::Result<Vec<u8>> {
        self.state
            .lock()
            .map_err(|_error| io::Error::other("fake transport lock poisoned"))?
            .latest_input
            .clone()
            .ok_or_else(|| io::Error::other("missing latest input"))
    }
}

impl ClientTransport for FakeTransport {
    type Error = io::Error;

    fn send_control(&mut self, frame: Vec<u8>) -> Result<(), Self::Error> {
        self.state
            .lock()
            .map_err(|_error| io::Error::other("fake transport lock poisoned"))?
            .controls
            .push(frame);
        Ok(())
    }

    fn set_latest_input(&mut self, datagram: Vec<u8>) -> Result<(), Self::Error> {
        self.state
            .lock()
            .map_err(|_error| io::Error::other("fake transport lock poisoned"))?
            .latest_input = Some(datagram);
        Ok(())
    }

    fn send_time_sync(&mut self, _datagram: Vec<u8>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<ClientTransportEvent>, Self::Error> {
        Ok(self
            .state
            .lock()
            .map_err(|_error| io::Error::other("fake transport lock poisoned"))?
            .incoming
            .pop_front())
    }
}

#[derive(Debug, Default)]
struct FakePrediction {
    tick: SimulationTick,
    state: Option<u64>,
    reject_next_control: bool,
}

impl ClientPrediction for FakePrediction {
    type State = u64;
    type Error = io::Error;

    fn current_tick(&self) -> SimulationTick {
        self.tick
    }

    fn bootstrap(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        self.tick = SimulationTick::new(snapshot.tick().get());
        self.state = Some(snapshot.tick().get());
        Ok(PredictionUpdate::Bootstrapped { tick: self.tick })
    }

    fn apply_snapshot(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        self.tick = SimulationTick::new(snapshot.tick().get());
        Ok(PredictionUpdate::Converged { tick: self.tick })
    }

    fn queue_control(
        &mut self,
        _frame: &blackflower_networking::ControlFrame,
    ) -> Result<(), Self::Error> {
        if std::mem::take(&mut self.reject_next_control) {
            return Err(io::Error::other("rejected control"));
        }
        Ok(())
    }

    fn advance_to(&mut self, target: SimulationTick) -> Result<(), Self::Error> {
        self.tick = target;
        Ok(())
    }

    fn predicted_state(&self) -> Option<&Self::State> {
        self.state.as_ref()
    }
}

#[derive(Debug, Default)]
struct CounterDriver {
    tick: PredictionTick,
    state: u64,
    passes: Vec<PredictionPass>,
}

impl PredictionDriver<u64, InputFrame<u64>> for CounterDriver {
    type Error = io::Error;

    fn current_tick(&self) -> u64 {
        self.tick.get()
    }

    fn restore_authoritative(&mut self, tick: u64, state: &u64) -> Result<(), Self::Error> {
        self.tick = PredictionTick::new(tick);
        self.state = *state;
        Ok(())
    }

    fn simulate_tick(
        &mut self,
        pass: PredictionPass,
        tick: u64,
        input: &InputFrame<u64>,
    ) -> Result<u64, Self::Error> {
        if tick != input.tick().get() {
            return Err(io::Error::other("gameplay tick does not match input frame"));
        }
        self.passes.push(pass);
        self.advance(input)
    }
}

impl CounterDriver {
    fn advance(&mut self, input: &InputFrame<u64>) -> Result<u64, io::Error> {
        self.tick = input.tick();
        self.state = self.state.saturating_add(*input.input());
        Ok(self.state)
    }
}

struct CounterCodec;

impl PredictionCodec<u64, u64> for CounterCodec {
    type Error = io::Error;

    fn decode_snapshot(
        &mut self,
        snapshot: &Snapshot,
    ) -> Result<AuthoritativeSnapshot<u64>, Self::Error> {
        Ok(AuthoritativeSnapshot {
            tick: PredictionTick::new(snapshot.tick().get()),
            acknowledged_input: None,
            state: snapshot_counter(snapshot)?,
        })
    }

    fn decode_input(
        &mut self,
        frame: &blackflower_networking::ControlFrame,
    ) -> Result<u64, Self::Error> {
        frame
            .payload
            .first()
            .copied()
            .map(u64::from)
            .ok_or_else(|| io::Error::other("missing counter input"))
    }

    fn neutral_input(&self) -> u64 {
        0
    }

    fn compare_states(&self, predicted: &u64, authoritative: &u64) -> PredictionStateComparison {
        PredictionStateComparison::from_within_tolerance(predicted == authoritative)
    }
}

fn counter_snapshot(tick: u64, value: u64) -> Result<Snapshot, Box<dyn StdError>> {
    let component = ComponentId::try_from_u16(1)?;
    let entity = ReplicatedEntityId::try_from_u64(1)?;
    let state = ComponentState::new(
        ComponentSampleTick::new(tick),
        ReplicationPriority::OwnerCorrection,
        value.to_le_bytes().to_vec(),
    )?;
    let mut builder = SnapshotBuilder::new(SnapshotTick::new(tick));
    builder.upsert_entity(entity, EntityState::new([(component, state)])?);
    Ok(builder.build()?)
}

fn snapshot_counter(snapshot: &Snapshot) -> io::Result<u64> {
    let entity =
        ReplicatedEntityId::try_from_u64(1).map_err(|error| io::Error::other(error.to_string()))?;
    let component =
        ComponentId::try_from_u16(1).map_err(|error| io::Error::other(error.to_string()))?;
    let bytes = snapshot
        .get(entity)
        .and_then(|state| state.get(component))
        .map(ComponentState::bytes)
        .ok_or_else(|| io::Error::other("missing counter state"))?;
    let encoded: [u8; 8] = bytes
        .try_into()
        .map_err(|_bytes| io::Error::other("invalid counter state"))?;
    Ok(u64::from_le_bytes(encoded))
}
