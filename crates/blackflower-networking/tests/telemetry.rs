use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::io;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use blackflower_networking::{
    ClockState, InputAction, QueueKind, ResyncAction, SnapshotAction, initialize_network_metrics,
    record_clock_sessions, record_inputs, record_queue_depth_delta, record_resync, record_snapshot,
};
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};

type TestResult = Result<(), Box<dyn StdError>>;

#[derive(Debug, Default)]
struct RecordingRecorder {
    counters: Mutex<BTreeMap<String, Arc<metrics::atomics::AtomicU64>>>,
    gauges: Mutex<BTreeMap<String, Arc<metrics::atomics::AtomicU64>>>,
}

impl RecordingRecorder {
    fn counter(&self, name: &str, label: (&str, &str)) -> Result<u64, io::Error> {
        self.counters
            .lock()
            .map_err(|_error| io::Error::other("counter recorder lock poisoned"))?
            .get(&metric_key(name, Some(label)))
            .map(|counter| counter.load(Ordering::Acquire))
            .ok_or_else(|| io::Error::other(format!("missing counter {name}")))
    }

    fn gauge(&self, name: &str, label: (&str, &str)) -> Result<f64, io::Error> {
        self.gauges
            .lock()
            .map_err(|_error| io::Error::other("gauge recorder lock poisoned"))?
            .get(&metric_key(name, Some(label)))
            .map(|gauge| f64::from_bits(gauge.load(Ordering::Acquire)))
            .ok_or_else(|| io::Error::other(format!("missing gauge {name}")))
    }
}

impl Recorder for RecordingRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        let Ok(mut counters) = self.counters.lock() else {
            return Counter::noop();
        };
        let counter = Arc::clone(
            counters
                .entry(metric_key_from_key(key))
                .or_insert_with(|| Arc::new(metrics::atomics::AtomicU64::new(0))),
        );
        Counter::from_arc(counter)
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        let Ok(mut gauges) = self.gauges.lock() else {
            return Gauge::noop();
        };
        let gauge = Arc::clone(
            gauges
                .entry(metric_key_from_key(key))
                .or_insert_with(|| Arc::new(metrics::atomics::AtomicU64::new(0))),
        );
        Gauge::from_arc(gauge)
    }

    fn register_histogram(&self, _key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        Histogram::noop()
    }
}

#[test]
fn network_metrics_keep_queue_contributions_and_lifecycle_actions_distinct() -> TestResult {
    let recorder = RecordingRecorder::default();

    metrics::with_local_recorder(&recorder, || {
        initialize_network_metrics();

        record_queue_depth_delta(QueueKind::Control, 0, 3);
        record_queue_depth_delta(QueueKind::Control, 0, 2);
        record_queue_depth_delta(QueueKind::Control, 3, 1);
        record_queue_depth_delta(QueueKind::Control, 2, 0);
        record_clock_sessions(ClockState::Synchronized, 2);
        record_inputs(InputAction::Submitted, 1);
        record_inputs(InputAction::Accepted, 2);
        record_snapshot(SnapshotAction::Applied);
        record_snapshot(SnapshotAction::Acknowledged);
        record_resync(ResyncAction::Requested);
        record_resync(ResyncAction::Started);
    });

    assert_gauges(&recorder)?;
    assert_input_counters(&recorder)?;
    assert_snapshot_and_resync_counters(&recorder)
}

fn assert_gauges(recorder: &RecordingRecorder) -> TestResult {
    let queue_depth = recorder.gauge("blackflower_network_queue_depth", ("queue", "control"))?;
    let synchronized = recorder.gauge(
        "blackflower_network_clock_sessions",
        ("state", "synchronized"),
    )?;
    assert!((queue_depth - 1.0).abs() <= f64::EPSILON);
    assert!((synchronized - 2.0).abs() <= f64::EPSILON);
    Ok(())
}

fn assert_input_counters(recorder: &RecordingRecorder) -> TestResult {
    assert_eq!(
        recorder.counter("blackflower_network_inputs_total", ("action", "submitted"))?,
        1
    );
    assert_eq!(
        recorder.counter("blackflower_network_inputs_total", ("action", "accepted"))?,
        2
    );
    Ok(())
}

fn assert_snapshot_and_resync_counters(recorder: &RecordingRecorder) -> TestResult {
    assert_eq!(
        recorder.counter("blackflower_network_snapshots_total", ("action", "applied"))?,
        1
    );
    assert_eq!(
        recorder.counter(
            "blackflower_network_snapshots_total",
            ("action", "acknowledged")
        )?,
        1
    );
    assert_eq!(
        recorder.counter("blackflower_network_resync_total", ("action", "requested"))?,
        1
    );
    assert_eq!(
        recorder.counter("blackflower_network_resync_total", ("action", "started"))?,
        1
    );
    Ok(())
}

fn metric_key_from_key(key: &Key) -> String {
    let mut labels = key
        .labels()
        .map(|label| (label.key(), label.value()))
        .collect::<Vec<_>>();
    labels.sort_unstable();
    let labels = labels
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}{{{labels}}}", key.name())
}

fn metric_key(name: &str, label: Option<(&str, &str)>) -> String {
    let labels = label.map_or_else(String::new, |(name, value)| format!("{name}={value}"));
    format!("{name}{{{labels}}}")
}
