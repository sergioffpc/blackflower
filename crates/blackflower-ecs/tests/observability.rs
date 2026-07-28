#![cfg(feature = "metrics")]

use std::collections::BTreeSet;
use std::error::Error as StdError;
use std::io;
use std::sync::Mutex;

use blackflower_ecs::{BuiltinPhase, Component, Read, TickDelta, World};
use bytemuck::{Pod, Zeroable};
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};

type TestResult = Result<(), Box<dyn StdError>>;

#[derive(Debug, Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct Observed(u32);

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

#[test]
fn metrics_feature_emits_lifecycle_tick_stats_and_failures() -> TestResult {
    let recorder = RecordingRecorder::default();

    metrics::with_local_recorder(&recorder, || -> TestResult {
        let mut world = World::new()?;
        let observed = world.register_component::<Observed>()?;
        let entity = world.spawn()?;
        world.insert(entity, observed, Observed(1))?;
        let delta = TickDelta::from_seconds(1.0 / 60.0)?;
        let _should_continue = world.progress(delta)?;

        let update_phase = world.builtin_phase(BuiltinPhase::OnUpdate);
        world
            .system("ObservedFailure", "Observed")?
            .phase(update_phase)?
            .project(Read::<Observed>::field(0))?
            .each(|_context, _entity, _observed| {
                Err(io::Error::other("intentional observability failure").into())
            })?;
        let failure = world.progress(delta);
        assert!(failure.is_err());
        Ok(())
    })?;

    let names = recorder.names()?;

    for expected in [
        "blackflower_ecs_active_worlds",
        "blackflower_ecs_allocations_outstanding",
        "blackflower_ecs_callback_failures_total",
        "blackflower_ecs_entities",
        "blackflower_ecs_queries",
        "blackflower_ecs_registrations_total",
        "blackflower_ecs_systems",
        "blackflower_ecs_tables",
        "blackflower_ecs_tick_commands",
        "blackflower_ecs_tick_duration_seconds",
        "blackflower_ecs_tick_flecs_system_time_seconds",
        "blackflower_ecs_tick_systems_ran",
        "blackflower_ecs_ticks_total",
    ] {
        assert!(names.contains(expected), "missing metric {expected}");
    }
    Ok(())
}
