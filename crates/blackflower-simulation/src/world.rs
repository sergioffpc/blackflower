use blackflower_ecs::{Error, PhaseId, RunError, TickDelta, World};

use crate::telemetry::TickObservation;
use crate::{SIMULATION_TICK_RATE_HZ, SimulationPhase, SimulationPipeline, systems, telemetry};

/// Fixed duration, in seconds, of one authoritative simulation tick.
pub const SIMULATION_TICK_DELTA_SECONDS: f32 = 1.0 / 240.0;

const _: () = assert!(SIMULATION_TICK_RATE_HZ == 240);

/// Dedicated ECS world for the authoritative fixed-tick simulation.
///
/// Construction registers the complete [`SimulationPipeline`]. The owner may
/// then register components and systems through [`Self::ecs_mut`] before
/// advancing exactly one fixed step at a time with [`Self::tick`].
///
/// Wall-clock pacing and all input/output transport stay outside this type.
pub struct SimulationWorld {
    ecs: World,
    pipeline: SimulationPipeline,
    tick_delta: TickDelta,
}

impl SimulationWorld {
    /// Create a single-threaded authoritative simulation world.
    pub fn new() -> Result<Self, Error> {
        Self::from_ecs(World::new()?)
    }

    /// Turn an existing, independently configured ECS world into a simulation world.
    ///
    /// This supports configurations such as a persistent ECS worker pool
    /// created through [`World::builder`].
    pub fn from_ecs(mut ecs: World) -> Result<Self, Error> {
        let pipeline = SimulationPipeline::register(&mut ecs)?;
        systems::register(&mut ecs, pipeline)?;
        let tick_delta = TickDelta::from_seconds(SIMULATION_TICK_DELTA_SECONDS)?;
        telemetry::describe_metrics();
        Ok(Self {
            ecs,
            pipeline,
            tick_delta,
        })
    }

    /// Return the underlying ECS world.
    #[must_use]
    pub const fn ecs(&self) -> &World {
        &self.ecs
    }

    /// Return the underlying ECS world for setup or direct state access.
    #[must_use]
    pub const fn ecs_mut(&mut self) -> &mut World {
        &mut self.ecs
    }

    /// Return the registered authoritative simulation pipeline.
    #[must_use]
    pub const fn pipeline(&self) -> SimulationPipeline {
        self.pipeline
    }

    /// Return the world-bound handle for one simulation phase.
    #[must_use]
    pub const fn phase(&self, phase: SimulationPhase) -> PhaseId {
        self.pipeline.phase(phase)
    }

    /// Return the validated fixed-step delta used by [`Self::tick`].
    #[must_use]
    pub const fn tick_delta(&self) -> TickDelta {
        self.tick_delta
    }

    /// Advance the authoritative pipeline by exactly one 240 Hz tick.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "blackflower_simulation",
            name = "simulation_tick",
            level = "info",
            skip_all,
            fields(
                delta_seconds = f64::from(self.tick_delta.as_seconds()),
                result = tracing::field::Empty,
            ),
        )
    )]
    pub fn tick(&mut self) -> Result<bool, RunError> {
        let observation = TickObservation::start(self.tick_delta);
        let result = self.ecs.progress(self.tick_delta);
        observation.finish(&result);

        #[cfg(feature = "profiling")]
        profiling::finish_frame!();

        result
    }
}
