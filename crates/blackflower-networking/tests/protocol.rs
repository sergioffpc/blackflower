use std::error::Error as StdError;
use std::num::NonZeroU64;
use std::time::Duration;

use blackflower_networking::{
    AdmissionClaims, BandwidthScheduler, BudgetTier, ClientSession, ClockFilter, ClockSafety,
    ClockSample, CommandId, CommandTimingClass, CompatibilityContract, ConnectionEpoch,
    ControlFrame, DatagramHeader, Deduplication, DeduplicationError, DiscreteCommand, FlowId,
    FlowSequence, InputDeduplicator, InputHealth, InputSequence, MatchEgressBudget, MatchId,
    NetworkQueues, PlayerId, ProjectionDigest, ProtocolRevision, RequiredContentSetId,
    SessionControlMessage, SessionId, SessionState, SimulationCompatibilityId, SimulationTick,
    SnapshotAppliedAck, TimeSyncSchedule, TrafficClass, TrafficDirection, WireError,
    activation_tick, classify_command, decode_datagram, decode_input_datagram,
    decode_stream_preamble, encode_control_message, encode_datagram, encode_input_datagram,
    encode_stream_preamble, input_health,
};

type TestResult = Result<(), Box<dyn StdError>>;

#[test]
fn common_datagram_header_has_the_exact_eleven_byte_golden() -> TestResult {
    let bytes = encode_datagram(
        DatagramHeader {
            flow: FlowId::Input,
            connection_epoch: ConnectionEpoch::new(0x0102_0304),
            flow_sequence: FlowSequence::new(0x0506_0708),
        },
        &[0xaa, 0xbb],
    );
    assert_eq!(bytes, [1, 2, 4, 3, 2, 1, 8, 7, 6, 5, 0, 0xaa, 0xbb]);
    assert_eq!(decode_datagram(&bytes)?.payload, &[0xaa, 0xbb]);

    let mut reserved = bytes.clone();
    reserved[10] = 1;
    assert_eq!(decode_datagram(&reserved), Err(WireError::Reserved));
    assert_eq!(decode_datagram(&bytes[..10]), Err(WireError::Truncated));
    Ok(())
}

#[test]
fn stream_preamble_and_control_framing_reject_noncanonical_input() -> TestResult {
    let preamble = encode_stream_preamble(blackflower_networking::StreamKind::SessionControl);
    assert_eq!(preamble, [1, 1, 0, 0]);
    assert_eq!(
        decode_stream_preamble(&[1, 1, 0, 1]),
        Err(WireError::Reserved)
    );

    let message = SessionControlMessage::AdmissionRequest {
        ticket: vec![1, 2, 3],
    };
    let encoded = encode_control_message(&message)?;
    assert_eq!(encoded, [1, 5, 3, 0, 1, 2, 3]);
    let mut trailing = encoded;
    trailing.push(0);
    assert_eq!(
        blackflower_networking::decode_control_message(&trailing),
        Err(WireError::Trailing)
    );
    assert_eq!(
        blackflower_networking::decode_frame(&[1, 0x40, 0x03, 1, 2, 3], 16),
        Err(WireError::InvalidValue("non-canonical QUIC varint"))
    );
    Ok(())
}

#[test]
fn input_codec_and_deduplication_require_exact_redundant_bytes() -> TestResult {
    let frame = ControlFrame {
        sequence: InputSequence::new(10),
        execute_tick: SimulationTick::new(100),
        payload: vec![7, 8],
    };
    let input = blackflower_networking::InputDatagram {
        control_epoch: 3,
        controlled_entity: NonZeroU64::new(9).ok_or("controlled entity must be non-zero")?,
        frames: vec![frame.clone()],
        commands: Vec::new(),
        applied_snapshot: Some(SnapshotAppliedAck {
            snapshot_tick: SimulationTick::new(96),
            projection_digest: ProjectionDigest::from_bytes([4; 32]),
        }),
    };
    let encoded = encode_input_datagram(&input)?;
    assert_eq!(decode_input_datagram(&encoded)?, input);
    let mut invalid_redundancy = input.clone();
    invalid_redundancy.frames.push(ControlFrame {
        sequence: InputSequence::new(8),
        execute_tick: SimulationTick::new(96),
        payload: vec![5],
    });
    assert_eq!(
        encode_input_datagram(&invalid_redundancy),
        Err(WireError::InvalidValue("input frame redundancy"))
    );

    let mut deduplication = InputDeduplicator::default();
    assert_eq!(deduplication.observe_control(&frame)?, Deduplication::New);
    assert_eq!(
        deduplication.observe_control(&frame)?,
        Deduplication::Duplicate
    );
    let mut conflicting = frame;
    conflicting.payload.push(9);
    assert!(matches!(
        deduplication.observe_control(&conflicting),
        Err(DeduplicationError::ConflictingInput(sequence)) if sequence == InputSequence::new(10)
    ));
    Ok(())
}

#[test]
fn command_windows_and_input_failsafe_are_independent() {
    let command = DiscreteCommand {
        command_id: CommandId::new(1),
        origin_input_sequence: InputSequence::new(1),
        execute_tick: SimulationTick::new(80),
        view_tick: Some(SimulationTick::new(80)),
        timing_class: CommandTimingClass::RewindRay,
        kind: 1,
        payload: Vec::new(),
    };
    assert!(matches!(
        classify_command(SimulationTick::new(100), &command, true),
        blackflower_networking::CommandTimingDecision::Deliver {
            historical_tick: Some(tick),
            ..
        } if tick == SimulationTick::new(80)
    ));
    assert_eq!(
        input_health(SimulationTick::new(112), SimulationTick::new(100)),
        InputHealth::Holding
    );
    assert_eq!(
        input_health(SimulationTick::new(113), SimulationTick::new(100)),
        InputHealth::Neutralized
    );
    assert_eq!(
        input_health(SimulationTick::new(340), SimulationTick::new(100)),
        InputHealth::Failsafe
    );
}

#[test]
fn clock_filter_and_schedule_apply_admission_and_degraded_thresholds() -> TestResult {
    let mut filter = ClockFilter::new();
    filter.observe(
        ClockSample {
            client_send_micros: 1_000,
            server_receive_micros: 1_100,
            server_send_micros: 1_100,
            client_receive_micros: 1_200,
        },
        Duration::from_millis(1),
    )?;
    assert_eq!(
        filter.safety(Duration::from_millis(2)),
        ClockSafety::ActivationReady
    );

    let mut degraded = ClockFilter::new();
    for index in 0..3 {
        degraded.observe(
            ClockSample {
                client_send_micros: 0,
                server_receive_micros: 20_000,
                server_send_micros: 20_000,
                client_receive_micros: 40_000,
            },
            Duration::from_millis(index + 1),
        )?;
    }
    assert_eq!(
        degraded.safety(Duration::from_millis(4)),
        ClockSafety::Degraded
    );

    let mut schedule = TimeSyncSchedule::admission(Duration::ZERO);
    for index in 0..8 {
        assert!(schedule.take_due(Duration::from_millis(index * 100)));
    }
    assert!(!schedule.take_due(Duration::from_millis(800)));
    schedule.set_active(Duration::from_millis(800));
    schedule.path_changed(Duration::from_secs(1));
    for index in 0..8 {
        assert!(schedule.take_due(Duration::from_millis(1_000 + index * 100)));
    }
    assert!(!schedule.take_due(Duration::from_millis(1_800)));
    assert!(schedule.take_due(Duration::from_millis(2_700)));
    Ok(())
}

#[test]
fn exact_compatibility_activation_and_resync_limit_drive_session_state() -> TestResult {
    let contract = CompatibilityContract {
        protocol_revision: ProtocolRevision::V1,
        simulation_compatibility_id: SimulationCompatibilityId::from_bytes([2; 32]),
        required_content_set_id: RequiredContentSetId::from_bytes([3; 32]),
    };
    let mut session = ClientSession::new(contract, ConnectionEpoch::new(1));
    session.secure()?;
    session.authenticate()?;
    session.accept_claims(&claims(contract))?;
    session.synchronize()?;
    let activation = activation_tick(SimulationTick::new(100), 2);
    session.schedule_activation(SimulationTick::new(100), activation)?;
    assert!(!session.advance(SimulationTick::new(123))?);
    assert!(session.advance(activation)?);
    assert_eq!(session.state(), SessionState::Active);
    session.begin_resync(Duration::ZERO)?;
    session.synchronize()?;
    session.schedule_activation(SimulationTick::new(200), SimulationTick::new(224))?;
    assert!(session.advance(SimulationTick::new(224))?);
    session.begin_resync(Duration::from_secs(1))?;
    session.synchronize()?;
    session.schedule_activation(SimulationTick::new(300), SimulationTick::new(324))?;
    assert!(session.advance(SimulationTick::new(324))?);
    assert!(session.begin_resync(Duration::from_secs(2)).is_err());
    session.reconnect(ConnectionEpoch::new(2))?;
    assert_eq!(session.state(), SessionState::Synchronizing);
    assert_eq!(session.connection_epoch(), ConnectionEpoch::new(2));
    assert_eq!(session.scheduled_activation(), None);
    Ok(())
}

#[test]
fn bounded_scheduler_preserves_priority_and_reconciles_budget() -> TestResult {
    let mut queues = NetworkQueues::new();
    queues.push_snapshot(vec![3]);
    queues.set_latest_input(vec![2]);
    queues.push_control(vec![1])?;
    assert_eq!(
        queues.pop_scheduled(true).ok_or("missing control")?.class,
        TrafficClass::SessionControl
    );
    assert_eq!(
        queues.pop_scheduled(true).ok_or("missing input")?.class,
        TrafficClass::Input
    );
    assert_eq!(
        queues.pop_scheduled(true).ok_or("missing snapshot")?.class,
        TrafficClass::MinimumSnapshot
    );

    let mut budget = BandwidthScheduler::new(BudgetTier::Constrained, Duration::ZERO);
    assert!(budget.reserve(TrafficDirection::Upstream, 1_000, Duration::ZERO));
    budget.reconcile_udp_bytes(TrafficDirection::Upstream, 1_000, 1_100);

    let mut match_budget = MatchEgressBudget::new(BudgetTier::Constrained, Duration::ZERO);
    let mut peers =
        (0..32).map(|_index| BandwidthScheduler::new(BudgetTier::Constrained, Duration::ZERO));
    for _index in 0..31 {
        let mut peer = peers.next().ok_or("missing peer budget")?;
        assert!(peer.reserve_downstream(&mut match_budget, 64_000, Duration::ZERO));
    }
    let mut final_peer = peers.next().ok_or("missing final peer budget")?;
    assert!(!final_peer.reserve_downstream(&mut match_budget, 64_000, Duration::ZERO));
    Ok(())
}

fn claims(contract: CompatibilityContract) -> AdmissionClaims {
    AdmissionClaims {
        session_id: SessionId::from_bytes([1; 16]),
        player_id: PlayerId::from_bytes([2; 16]),
        match_id: MatchId::from_bytes([3; 16]),
        protocol_revision: contract.protocol_revision,
        simulation_compatibility_id: contract.simulation_compatibility_id,
        required_content_set_id: contract.required_content_set_id,
    }
}
