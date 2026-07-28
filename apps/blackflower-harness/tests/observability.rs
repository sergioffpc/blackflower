use std::collections::BTreeSet;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use blackflower_ecs::TickDelta;
use blackflower_prediction::{PredictionPass, PredictionTick, PredictionWorld};
use blackflower_presentation::{FrameIndex, PresentationWorld};
use blackflower_simulation::SimulationWorld;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata as TracingMetadata, Subscriber};

type TestResult = Result<()>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScenarioResult {
    simulation_continued: bool,
    prediction_tick: PredictionTick,
    presentation_frame: FrameIndex,
}

#[derive(Debug, Default)]
struct RecordingRecorder {
    names: Mutex<BTreeSet<String>>,
}

impl RecordingRecorder {
    fn record(&self, key: &Key) {
        if let Ok(mut names) = self.names.lock() {
            names.insert(key.name().to_string());
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

#[derive(Clone)]
struct CountingSubscriber {
    events: Arc<AtomicUsize>,
    spans: Arc<AtomicUsize>,
}

impl Subscriber for CountingSubscriber {
    fn enabled(&self, _metadata: &TracingMetadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _attributes: &Attributes<'_>) -> Id {
        let id = self.spans.fetch_add(1, Ordering::Relaxed) + 1;
        Id::from_u64(u64::try_from(id).unwrap_or(u64::MAX))
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, _event: &Event<'_>) {
        self.events.fetch_add(1, Ordering::Relaxed);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

#[test]
fn observability_emits_signals_without_changing_world_results() -> TestResult {
    let baseline = run_scenario()?;
    let recorder = RecordingRecorder::default();
    let events = Arc::new(AtomicUsize::new(0));
    let spans = Arc::new(AtomicUsize::new(0));
    let subscriber = CountingSubscriber {
        events: Arc::clone(&events),
        spans: Arc::clone(&spans),
    };

    let observed = tracing::subscriber::with_default(subscriber, || {
        metrics::with_local_recorder(&recorder, run_scenario)
    })?;

    assert_eq!(observed, baseline);
    assert!(spans.load(Ordering::Relaxed) >= 6);
    assert!(events.load(Ordering::Relaxed) >= 6);

    let names = recorder.names()?;
    for expected in [
        "blackflower_ecs_ticks_total",
        "blackflower_prediction_tick_duration_seconds",
        "blackflower_prediction_ticks_total",
        "blackflower_presentation_frame_delta_seconds",
        "blackflower_presentation_frame_duration_seconds",
        "blackflower_presentation_frames_total",
        "blackflower_simulation_tick_duration_seconds",
        "blackflower_simulation_ticks_total",
    ] {
        assert!(names.contains(expected), "missing metric {expected}");
    }
    Ok(())
}

fn run_scenario() -> Result<ScenarioResult> {
    let mut simulation = SimulationWorld::new()?;
    let simulation_continued = simulation.tick()?;

    let mut prediction = PredictionWorld::new()?;
    let _prediction_continued = prediction.tick(PredictionPass::Forward)?;

    let mut presentation = PresentationWorld::new()?;
    let _presentation_continued = presentation.frame(TickDelta::from_seconds(1.0 / 60.0)?)?;

    Ok(ScenarioResult {
        simulation_continued,
        prediction_tick: prediction.current_tick(),
        presentation_frame: presentation.current_frame(),
    })
}
