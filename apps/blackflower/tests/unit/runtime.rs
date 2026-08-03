use std::collections::VecDeque;
use std::io;
use std::time::{Duration, Instant};

use blackflower_ecs::TickDelta;
use blackflower_harness::{
    ClientEvent, ClientHarness, ClientHarnessConfig, ClientPrediction, ClientTransport,
    ClientTransportEvent, ClientView, PredictionUpdate,
};
use blackflower_networking::{
    CompatibilityContract, ConnectionEpoch, ProtocolRevision, RequiredContentSetId, SessionState,
    SimulationCompatibilityId, SimulationTick,
};
use blackflower_networking_replication::Snapshot;
use blackflower_world_presentation::PresentationWorld;

use super::{FrameClock, HarnessPresentationRuntime, PresentationBridge};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn frame_clock_resets_and_clamps_long_suspension() -> TestResult {
    let started = Instant::now();
    let mut clock = FrameClock::default();
    clock.resume(started);
    let normal = clock.delta(started + Duration::from_millis(10))?;
    assert_eq!(normal.as_seconds().to_bits(), 0.01_f32.to_bits());

    let clamped = clock.delta(started + Duration::from_secs(2))?;
    assert_eq!(clamped.as_seconds().to_bits(), 0.25_f32.to_bits());

    clock.suspend();
    let resumed = clock.delta(started + Duration::from_secs(10))?;
    assert_eq!(resumed.as_seconds().to_bits(), (1.0_f32 / 60.0).to_bits());
    Ok(())
}

#[test]
fn harness_view_is_captured_before_presentation_advances() -> TestResult {
    let harness = ClientHarness::new(
        TestTransport::default(),
        TestPrediction::default(),
        harness_config(),
    )?;
    let mut runtime = HarnessPresentationRuntime::new(harness, TestBridge::default())?;

    assert!(runtime.frame(
        Duration::from_millis(1),
        TickDelta::from_seconds(1.0 / 60.0)?,
    )?);

    let capture = runtime.bridge();
    assert_eq!(capture.session_state, Some(SessionState::Authenticating));
    assert_eq!(capture.authoritative_count, 0);
    assert!(!capture.predicted);
    assert_eq!(capture.event_count, 0);
    assert_eq!(runtime.presentation().current_frame().get(), 1);
    Ok(())
}

fn harness_config() -> ClientHarnessConfig {
    ClientHarnessConfig {
        compatibility: CompatibilityContract {
            protocol_revision: ProtocolRevision::V1,
            simulation_compatibility_id: SimulationCompatibilityId::from_bytes([1; 32]),
            required_content_set_id: RequiredContentSetId::from_bytes([2; 32]),
        },
        connection_epoch: ConnectionEpoch::new(1),
        admission_ticket: b"ticket".to_vec(),
    }
}

#[derive(Default)]
struct TestTransport {
    events: VecDeque<ClientTransportEvent>,
    control: Vec<Vec<u8>>,
    latest_input: Option<Vec<u8>>,
}

impl ClientTransport for TestTransport {
    type Error = io::Error;

    fn send_control(&mut self, frame: Vec<u8>) -> Result<(), Self::Error> {
        self.control.push(frame);
        Ok(())
    }

    fn set_latest_input(&mut self, datagram: Vec<u8>) -> Result<(), Self::Error> {
        self.latest_input = Some(datagram);
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<ClientTransportEvent>, Self::Error> {
        Ok(self.events.pop_front())
    }
}

#[derive(Default)]
struct TestPrediction {
    tick: SimulationTick,
    state: Option<u64>,
}

impl ClientPrediction for TestPrediction {
    type State = u64;
    type Error = io::Error;

    fn current_tick(&self) -> SimulationTick {
        self.tick
    }

    fn bootstrap(&mut self, _snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        Ok(PredictionUpdate::Bootstrapped { tick: self.tick })
    }

    fn apply_snapshot(&mut self, _snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        Ok(PredictionUpdate::Converged { tick: self.tick })
    }

    fn queue_control(
        &mut self,
        _frame: &blackflower_networking::ControlFrame,
    ) -> Result<(), Self::Error> {
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

#[derive(Default)]
struct TestBridge {
    session_state: Option<SessionState>,
    authoritative_count: usize,
    predicted: bool,
    event_count: usize,
}

impl PresentationBridge<u64> for TestBridge {
    type Error = io::Error;

    fn capture(
        &mut self,
        presentation: &mut PresentationWorld,
        view: ClientView<'_, u64>,
        events: &[ClientEvent],
    ) -> Result<(), Self::Error> {
        if presentation.current_frame().get() != 0 {
            return Err(io::Error::other("presentation advanced before capture"));
        }
        self.session_state = Some(view.session_state());
        self.authoritative_count = view.authoritative_window().len();
        self.predicted = view.predicted().is_some();
        self.event_count = events.len();
        Ok(())
    }
}
