use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use blackflower_ecs::{Error, PhaseId, RunError, TickDelta, World};

use crate::telemetry::TickObservation;
use crate::types::SimulationTickOverflow;
use crate::{
    SIMULATION_TICK_RATE_HZ, SimulationPhase, SimulationPipeline, SimulationTick, systems,
    telemetry,
};

/// Fixed duration, in seconds, of one authoritative simulation tick.
pub const SIMULATION_TICK_DELTA_SECONDS: f32 = 1.0 / 240.0;

const _: () = assert!(SIMULATION_TICK_RATE_HZ == 240);

#[derive(Debug)]
struct ExecutionState {
    tick: AtomicU64,
}

/// Snapshot of the authoritative simulation execution visible to systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationExecution {
    /// Tick currently being executed.
    pub tick: SimulationTick,
}

/// Shared read-only execution context that simulation systems may capture.
///
/// During pipeline execution, [`Self::current`] returns the tick opened by
/// `OpenTick`. Outside execution it returns the latest successfully completed
/// tick.
#[derive(Debug, Clone)]
pub struct SimulationExecutionContext {
    state: Arc<ExecutionState>,
}

impl SimulationExecutionContext {
    fn new() -> Self {
        Self {
            state: Arc::new(ExecutionState {
                tick: AtomicU64::new(SimulationTick::ZERO.get()),
            }),
        }
    }

    /// Return the authoritative tick active in this execution context.
    #[must_use]
    pub fn current(&self) -> SimulationExecution {
        SimulationExecution {
            tick: SimulationTick::new(self.state.tick.load(Ordering::Acquire)),
        }
    }

    pub(crate) fn open_next(&self) -> Result<SimulationExecution, SimulationTickOverflow> {
        let tick = self
            .current()
            .tick
            .checked_next()
            .ok_or(SimulationTickOverflow)?;
        let execution = SimulationExecution { tick };
        self.set(execution);
        Ok(execution)
    }

    fn set(&self, execution: SimulationExecution) {
        self.state
            .tick
            .store(execution.tick.get(), Ordering::Release);
    }
}

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
    execution_context: SimulationExecutionContext,
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
        let execution_context = SimulationExecutionContext::new();
        systems::register(&mut ecs, pipeline, execution_context.clone())?;
        let tick_delta = TickDelta::from_seconds(SIMULATION_TICK_DELTA_SECONDS)?;
        telemetry::describe_metrics();
        Ok(Self {
            ecs,
            pipeline,
            tick_delta,
            execution_context,
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

    /// Return the latest successfully completed authoritative tick.
    #[must_use]
    pub fn current_tick(&self) -> SimulationTick {
        self.execution_context.current().tick
    }

    /// Return a context handle for systems that need the active tick.
    #[must_use]
    pub fn execution_context(&self) -> SimulationExecutionContext {
        self.execution_context.clone()
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
                tick = tracing::field::Empty,
                delta_seconds = f64::from(self.tick_delta.as_seconds()),
                result = tracing::field::Empty,
            ),
        )
    )]
    pub fn tick(&mut self) -> Result<bool, RunError> {
        let previous_execution = self.execution_context.current();
        let observation = TickObservation::start(self.tick_delta);
        let run_result = self.ecs.progress(self.tick_delta);
        #[cfg(feature = "tracing")]
        tracing::Span::current().record("tick", self.execution_context.current().tick.get());
        let result = match run_result {
            Ok(should_continue) => Ok(should_continue),
            Err(error) => {
                self.execution_context.set(previous_execution);
                Err(error)
            }
        };
        observation.finish(&result);

        #[cfg(feature = "profiling")]
        profiling::finish_frame!();

        result
    }
}
