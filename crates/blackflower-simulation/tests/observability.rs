#![cfg(all(feature = "metrics", feature = "tracing"))]

use std::error::Error as StdError;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use blackflower_simulation::SimulationWorld;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata as TracingMetadata, Subscriber};

type TestResult = Result<(), Box<dyn StdError>>;

#[derive(Debug, Default)]
struct RecordingRecorder {
    system_executions: Arc<AtomicU64>,
}

impl Recorder for RecordingRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        if key.name() == "blackflower_simulation_system_executions_total" {
            Counter::from_arc(Arc::clone(&self.system_executions))
        } else {
            Counter::noop()
        }
    }

    fn register_gauge(&self, _key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        Gauge::noop()
    }

    fn register_histogram(&self, _key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        Histogram::noop()
    }
}

#[derive(Clone)]
struct CountingSubscriber {
    simulation_events: Arc<AtomicUsize>,
    next_span_id: Arc<AtomicU64>,
}

impl Subscriber for CountingSubscriber {
    fn enabled(&self, _metadata: &TracingMetadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _attributes: &Attributes<'_>) -> Id {
        let id = self.next_span_id.fetch_add(1, Ordering::Relaxed) + 1;
        Id::from_u64(id)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        if event.metadata().target() == "blackflower_simulation" {
            self.simulation_events.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

#[test]
fn prepare_tick_stubs_emit_observability_signals() -> TestResult {
    let recorder = RecordingRecorder::default();
    let system_executions = Arc::clone(&recorder.system_executions);
    let simulation_events = Arc::new(AtomicUsize::new(0));
    let subscriber = CountingSubscriber {
        simulation_events: Arc::clone(&simulation_events),
        next_span_id: Arc::new(AtomicU64::new(0)),
    };

    tracing::subscriber::with_default(subscriber, || {
        metrics::with_local_recorder(&recorder, || -> TestResult {
            let mut simulation = SimulationWorld::new()?;
            assert!(simulation.tick()?);
            Ok(())
        })
    })?;

    assert_eq!(system_executions.load(Ordering::Relaxed), 2);
    assert!(simulation_events.load(Ordering::Relaxed) >= 2);
    Ok(())
}
