#[cfg(feature = "metrics")]
use crate::ffi;
use crate::ffi::WorldPtr;
use crate::ids::{TickDelta, WorldKey};

#[cfg(feature = "tracing")]
const TARGET: &str = "blackflower_ecs";

#[derive(Debug, Clone, Copy)]
pub(crate) enum ResourceKind {
    Component,
    Tag,
    System,
    Pipeline,
    Phase,
}

impl ResourceKind {
    #[cfg(any(feature = "metrics", feature = "tracing"))]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Component => "component",
            Self::Tag => "tag",
            Self::System => "system",
            Self::Pipeline => "pipeline",
            Self::Phase => "phase",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CallbackFailureKind {
    Error,
    Panic,
    Projection,
    Internal,
}

impl CallbackFailureKind {
    #[cfg(any(feature = "metrics", feature = "tracing"))]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Panic => "panic",
            Self::Projection => "projection",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TickOutcome {
    Completed,
    Continued,
    Stopped,
    Failed,
}

impl TickOutcome {
    #[cfg(any(feature = "metrics", feature = "tracing"))]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Continued => "continued",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }
}

#[derive(Default)]
pub(crate) struct State {
    #[cfg(feature = "metrics")]
    stats: ffi::WorldStatsState,
    #[cfg(feature = "metrics")]
    contribution: GaugeContribution,
}

impl State {
    pub(crate) fn prime(&mut self, world: WorldPtr) {
        #[cfg(feature = "metrics")]
        {
            describe_metrics();
            let sample = ffi::sample_world_stats(world, &mut self.stats);
            self.update_gauges(sample);
        }

        #[cfg(not(feature = "metrics"))]
        let _ = world;
    }

    pub(crate) fn report(&mut self, world: WorldPtr) {
        #[cfg(feature = "metrics")]
        {
            let sample = ffi::sample_world_stats(world, &mut self.stats);
            self.update_gauges(sample);
            emit_tick_distributions(sample);
        }

        #[cfg(not(feature = "metrics"))]
        let _ = world;
    }

    pub(crate) fn detach(&mut self) {
        #[cfg(feature = "metrics")]
        {
            update_aggregate_gauge("blackflower_ecs_entities", self.contribution.entities, 0.0);
            update_aggregate_gauge("blackflower_ecs_tables", self.contribution.tables, 0.0);
            update_aggregate_gauge("blackflower_ecs_queries", self.contribution.queries, 0.0);
            update_aggregate_gauge("blackflower_ecs_systems", self.contribution.systems, 0.0);
            self.contribution = GaugeContribution::default();
        }
    }

    #[cfg(feature = "metrics")]
    fn update_gauges(&mut self, sample: ffi::WorldStatsSample) {
        update_aggregate_gauge(
            "blackflower_ecs_entities",
            self.contribution.entities,
            sample.entities,
        );
        update_aggregate_gauge(
            "blackflower_ecs_tables",
            self.contribution.tables,
            sample.tables,
        );
        update_aggregate_gauge(
            "blackflower_ecs_queries",
            self.contribution.queries,
            sample.queries,
        );
        update_aggregate_gauge(
            "blackflower_ecs_systems",
            self.contribution.systems,
            sample.systems,
        );
        metrics::gauge!("blackflower_ecs_allocations_outstanding")
            .set(sample.allocations_outstanding);
        self.contribution = GaugeContribution {
            entities: sample.entities,
            tables: sample.tables,
            queries: sample.queries,
            systems: sample.systems,
        };
    }
}

#[cfg(feature = "metrics")]
#[derive(Debug, Default, Clone, Copy)]
struct GaugeContribution {
    entities: f64,
    tables: f64,
    queries: f64,
    systems: f64,
}

pub(crate) struct TickObservation {
    #[cfg(feature = "metrics")]
    operation: &'static str,
    #[cfg(feature = "metrics")]
    started: std::time::Instant,
    #[cfg(feature = "tracing")]
    span: tracing::Span,
}

impl TickObservation {
    pub(crate) fn start(
        operation: &'static str,
        world: WorldKey,
        delta: TickDelta,
        pipeline: Option<u64>,
    ) -> Self {
        #[cfg(not(feature = "tracing"))]
        let _ = (operation, world, delta, pipeline);

        Self {
            #[cfg(feature = "metrics")]
            operation,
            #[cfg(feature = "metrics")]
            started: std::time::Instant::now(),
            #[cfg(feature = "tracing")]
            span: tracing::trace_span!(
                target: TARGET,
                "ecs_tick",
                world_id = world.0,
                operation,
                delta_seconds = f64::from(delta.as_seconds()),
                pipeline,
            ),
        }
    }

    pub(crate) fn in_scope<R>(&self, callback: impl FnOnce() -> R) -> R {
        #[cfg(feature = "tracing")]
        {
            self.span.in_scope(callback)
        }

        #[cfg(not(feature = "tracing"))]
        {
            callback()
        }
    }

    pub(crate) fn finish(self, outcome: TickOutcome, state: &mut State, world: WorldPtr) {
        #[cfg(feature = "metrics")]
        {
            let elapsed = self.started.elapsed();
            metrics::counter!(
                "blackflower_ecs_ticks_total",
                "operation" => self.operation,
                "result" => outcome.as_str(),
            )
            .increment(1);
            metrics::histogram!(
                "blackflower_ecs_tick_duration_seconds",
                "operation" => self.operation,
            )
            .record(elapsed.as_secs_f64());
        }

        state.report(world);

        #[cfg(feature = "tracing")]
        self.span.in_scope(|| {
            tracing::trace!(
                target: TARGET,
                result = outcome.as_str(),
                "Flecs tick completed",
            );
        });

        #[cfg(not(feature = "tracing"))]
        let _ = outcome;
    }
}

pub(crate) fn world_created(world: WorldKey, workers: u32) {
    #[cfg(feature = "metrics")]
    metrics::gauge!("blackflower_ecs_active_worlds").increment(1.0);

    #[cfg(feature = "tracing")]
    tracing::debug!(
        target: TARGET,
        world_id = world.0,
        worker_threads = workers,
        "Flecs world created",
    );

    #[cfg(not(feature = "tracing"))]
    let _ = (world, workers);
}

pub(crate) fn world_destroyed(world: WorldKey, workers_started: bool) {
    #[cfg(feature = "metrics")]
    metrics::gauge!("blackflower_ecs_active_worlds").decrement(1.0);

    #[cfg(feature = "tracing")]
    tracing::debug!(
        target: TARGET,
        world_id = world.0,
        workers_started,
        "Flecs world shut down",
    );

    #[cfg(not(feature = "tracing"))]
    let _ = (world, workers_started);
}

pub(crate) fn resource_registered(
    world: WorldKey,
    kind: ResourceKind,
    name: &str,
    parallel: Option<bool>,
) {
    #[cfg(feature = "metrics")]
    metrics::counter!(
        "blackflower_ecs_registrations_total",
        "kind" => kind.as_str(),
    )
    .increment(1);

    #[cfg(feature = "tracing")]
    tracing::debug!(
        target: TARGET,
        world_id = world.0,
        kind = kind.as_str(),
        name,
        parallel,
        "Flecs resource registered",
    );

    #[cfg(not(feature = "tracing"))]
    let _ = (world, kind, name, parallel);
}

pub(crate) fn callback_failed(
    world: WorldKey,
    kind: CallbackFailureKind,
    system: &str,
    message: &str,
) {
    #[cfg(feature = "metrics")]
    metrics::counter!(
        "blackflower_ecs_callback_failures_total",
        "kind" => kind.as_str(),
    )
    .increment(1);

    #[cfg(feature = "tracing")]
    tracing::error!(
        target: TARGET,
        world_id = world.0,
        kind = kind.as_str(),
        system,
        error = message,
        "Rust system callback failed",
    );

    #[cfg(not(feature = "tracing"))]
    let _ = (world, kind, system, message);
}

pub(crate) fn rejected_pipeline(world: WorldKey, pipeline_world: WorldKey) {
    #[cfg(feature = "tracing")]
    tracing::warn!(
        target: TARGET,
        world_id = world.0,
        pipeline_world_id = pipeline_world.0,
        "pipeline belongs to another Flecs world",
    );

    #[cfg(not(feature = "tracing"))]
    let _ = (world, pipeline_world);
}

#[cfg(feature = "metrics")]
fn update_aggregate_gauge(name: &'static str, previous: f64, current: f64) {
    let gauge = metrics::gauge!(name);
    let delta = current - previous;
    if delta > 0.0 {
        gauge.increment(delta);
    } else if delta < 0.0 {
        gauge.decrement(-delta);
    }
}

#[cfg(feature = "metrics")]
fn describe_metrics() {
    describe_gauge_metrics();
    describe_counter_metrics();
    describe_tick_histograms();
    describe_flecs_histograms();
}

#[cfg(feature = "metrics")]
fn describe_gauge_metrics() {
    use metrics::Unit;

    metrics::describe_gauge!(
        "blackflower_ecs_active_worlds",
        Unit::Count,
        "Number of live blackflower-ecs worlds",
    );
    metrics::describe_gauge!(
        "blackflower_ecs_entities",
        Unit::Count,
        "Entities across live blackflower-ecs worlds",
    );
    metrics::describe_gauge!(
        "blackflower_ecs_tables",
        Unit::Count,
        "Flecs tables across live blackflower-ecs worlds",
    );
    metrics::describe_gauge!(
        "blackflower_ecs_queries",
        Unit::Count,
        "Flecs queries across live blackflower-ecs worlds",
    );
    metrics::describe_gauge!(
        "blackflower_ecs_systems",
        Unit::Count,
        "Flecs systems across live blackflower-ecs worlds",
    );
    metrics::describe_gauge!(
        "blackflower_ecs_allocations_outstanding",
        Unit::Count,
        "Outstanding allocations reported by the Flecs process",
    );
}

#[cfg(feature = "metrics")]
fn describe_counter_metrics() {
    use metrics::Unit;

    metrics::describe_counter!(
        "blackflower_ecs_ticks_total",
        Unit::Count,
        "Completed Flecs tick executions",
    );
    metrics::describe_counter!(
        "blackflower_ecs_registrations_total",
        Unit::Count,
        "Resources registered through the safe Rust API",
    );
    metrics::describe_counter!(
        "blackflower_ecs_callback_failures_total",
        Unit::Count,
        "First Rust callback failure recorded in a tick",
    );
}

#[cfg(feature = "metrics")]
fn describe_tick_histograms() {
    use metrics::Unit;

    metrics::describe_histogram!(
        "blackflower_ecs_tick_duration_seconds",
        Unit::Seconds,
        "Wall-clock duration of a Flecs tick",
    );
    metrics::describe_histogram!(
        "blackflower_ecs_tick_systems_ran",
        Unit::Count,
        "Flecs systems executed in a tick",
    );
    metrics::describe_histogram!(
        "blackflower_ecs_tick_merges",
        Unit::Count,
        "Flecs merges executed in a tick",
    );
    metrics::describe_histogram!(
        "blackflower_ecs_tick_rematches",
        Unit::Count,
        "Flecs query rematches performed in a tick",
    );
    metrics::describe_histogram!(
        "blackflower_ecs_tick_pipeline_rebuilds",
        Unit::Count,
        "Flecs pipeline rebuilds performed in a tick",
    );
}

#[cfg(feature = "metrics")]
fn describe_flecs_histograms() {
    use metrics::Unit;

    metrics::describe_histogram!(
        "blackflower_ecs_tick_flecs_frame_time_seconds",
        Unit::Seconds,
        "Wall-clock time measured inside Flecs for a tick",
    );
    metrics::describe_histogram!(
        "blackflower_ecs_tick_flecs_system_time_seconds",
        Unit::Seconds,
        "Wall-clock time measured by Flecs while executing systems",
    );
    metrics::describe_histogram!(
        "blackflower_ecs_tick_flecs_merge_time_seconds",
        Unit::Seconds,
        "Wall-clock time measured by Flecs while merging commands",
    );
    metrics::describe_histogram!(
        "blackflower_ecs_tick_flecs_rematch_time_seconds",
        Unit::Seconds,
        "Wall-clock time measured by Flecs while rematching queries",
    );
    metrics::describe_histogram!(
        "blackflower_ecs_tick_commands",
        Unit::Count,
        "Deferred Flecs commands processed in a tick",
    );
}

#[cfg(feature = "metrics")]
fn emit_tick_distributions(sample: ffi::WorldStatsSample) {
    metrics::histogram!("blackflower_ecs_tick_systems_ran").record(sample.systems_ran);
    metrics::histogram!("blackflower_ecs_tick_merges").record(sample.merges);
    metrics::histogram!("blackflower_ecs_tick_rematches").record(sample.rematches);
    metrics::histogram!("blackflower_ecs_tick_pipeline_rebuilds").record(sample.pipeline_rebuilds);
    metrics::histogram!("blackflower_ecs_tick_flecs_frame_time_seconds")
        .record(sample.frame_time_seconds);
    metrics::histogram!("blackflower_ecs_tick_flecs_system_time_seconds")
        .record(sample.system_time_seconds);
    metrics::histogram!("blackflower_ecs_tick_flecs_merge_time_seconds")
        .record(sample.merge_time_seconds);
    metrics::histogram!("blackflower_ecs_tick_flecs_rematch_time_seconds")
        .record(sample.rematch_time_seconds);

    for (operation, count) in [
        ("add", sample.command_adds),
        ("remove", sample.command_removes),
        ("delete", sample.command_deletes),
        ("clear", sample.command_clears),
        ("set", sample.command_sets),
        ("ensure", sample.command_ensures),
        ("modified", sample.command_modifications),
        ("other", sample.command_other),
        ("discard", sample.command_discards),
    ] {
        metrics::histogram!(
            "blackflower_ecs_tick_commands",
            "operation" => operation,
        )
        .record(count);
    }
}

#[cfg(all(test, feature = "tracing"))]
mod tests {
    use std::error::Error as StdError;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Metadata, Subscriber};

    use crate::{TickDelta, World};

    #[derive(Clone)]
    struct CountingSubscriber {
        events: Arc<AtomicUsize>,
        spans: Arc<AtomicUsize>,
    }

    impl Subscriber for CountingSubscriber {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
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
    fn tracing_feature_emits_lifecycle_and_tick_signals() -> Result<(), Box<dyn StdError>> {
        let events = Arc::new(AtomicUsize::new(0));
        let spans = Arc::new(AtomicUsize::new(0));
        let subscriber = CountingSubscriber {
            events: Arc::clone(&events),
            spans: Arc::clone(&spans),
        };

        tracing::subscriber::with_default(subscriber, || -> Result<(), Box<dyn StdError>> {
            let mut world = World::new()?;
            let _should_continue = world.progress(TickDelta::from_seconds(1.0 / 60.0)?)?;
            Ok(())
        })?;

        assert!(events.load(Ordering::Relaxed) >= 3);
        assert_eq!(spans.load(Ordering::Relaxed), 1);
        Ok(())
    }
}
