use std::collections::BTreeSet;
use std::io;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use blackflower_world_simulation::SIMULATION_TICK_RATE_HZ;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};

use super::{NANOSECONDS_PER_SECOND, SimulationHost, TickPacer, telemetry};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Debug, Default)]
struct RecordingRecorder {
    names: Mutex<BTreeSet<String>>,
}

impl RecordingRecorder {
    fn record(&self, key: &Key) {
        if let Ok(mut names) = self.names.lock() {
            names.insert(key.name().to_owned());
        }
    }

    fn names(&self) -> Result<BTreeSet<String>, io::Error> {
        self.names
            .lock()
            .map(|names| names.clone())
            .map_err(|_error| io::Error::other("metrics recorder lock poisoned"))
    }
}

impl Recorder for RecordingRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        self.record(key);
        Counter::noop()
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        self.record(key);
        Gauge::noop()
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        self.record(key);
        Histogram::noop()
    }
}

#[test]
fn pacer_accumulates_exactly_one_second_for_one_tick_rate_window() {
    let started = Instant::now();
    let mut pacer = TickPacer::new(started);
    for _tick in 0..SIMULATION_TICK_RATE_HZ {
        pacer.advance();
    }
    assert_eq!(
        pacer.deadline.duration_since(started).as_nanos(),
        u128::from(NANOSECONDS_PER_SECOND),
    );
}

#[test]
fn pacer_separates_wait_lag_catch_up_and_deadline_pressure() {
    let deadline = Instant::now();
    let pacer = TickPacer::new(deadline);
    let timing = pacer.timing_at(
        deadline + Duration::from_millis(10),
        Duration::from_millis(2),
    );

    assert_eq!(timing.waited, Duration::from_millis(2));
    assert_eq!(timing.lag, Duration::from_millis(10));
    assert_eq!(timing.ticks_behind, 2);
    let pressure = timing.deadline_pressure_ratio(deadline + Duration::from_millis(12));
    assert!((pressure - 2.88).abs() <= 1.0e-12);
}

#[test]
fn scheduler_emits_every_owned_metric_family() -> TestResult {
    let recorder = RecordingRecorder::default();
    metrics::with_local_recorder(&recorder, || {
        telemetry::initialize();
        telemetry::tick_started(Duration::from_millis(1), Duration::from_millis(10), 2);
        telemetry::tick_finished(2.88);
    });

    let names = recorder.names()?;
    for expected in [
        "blackflower_server_simulation_scheduler_wait_seconds",
        "blackflower_server_simulation_scheduler_tick_lag_seconds",
        "blackflower_server_simulation_scheduler_catch_up_depth_ticks",
        "blackflower_server_simulation_scheduler_catch_up_ticks_total",
        "blackflower_server_simulation_scheduler_deadline_pressure_ratio",
    ] {
        assert!(names.contains(expected), "missing metric {expected}");
    }
    Ok(())
}

#[test]
fn simulation_host_ticks_until_orderly_shutdown() -> TestResult {
    let host = SimulationHost::spawn()?;
    wait_for_ticks(&host, 2)?;
    let exit = host.shutdown()?;
    assert!(exit.completed_ticks >= 2);
    Ok(())
}

fn wait_for_ticks(host: &SimulationHost, expected: u64) -> TestResult {
    let deadline = Instant::now() + Duration::from_millis(500);
    while host.completed_ticks() < expected {
        if Instant::now() >= deadline {
            return Err(io::Error::other("simulation host did not advance").into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
