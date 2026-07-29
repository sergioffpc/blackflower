use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use blackflower_ecs::{Error, PhaseId, RunError, TickDelta, World};

use crate::telemetry;
use crate::telemetry::TickObservation;
use crate::{PredictionPass, PredictionPhase, PredictionPipeline, PredictionTick, systems};

/// Predicted simulation ticks executed per second.
pub const PREDICTION_TICK_RATE_HZ: u64 = 240;

/// Fixed duration, in seconds, of one predicted simulation tick.
pub const PREDICTION_TICK_DELTA_SECONDS: f32 = 1.0 / 240.0;

const _: () = assert!(PREDICTION_TICK_RATE_HZ == 240);

const FORWARD_PASS: u8 = 0;
const RESIMULATION_PASS: u8 = 1;

#[derive(Debug)]
struct ExecutionState {
    tick: AtomicU64,
    pass: AtomicU8,
}

/// Snapshot of the prediction execution visible to registered systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PredictionExecution {
    /// Tick currently being executed.
    pub tick: PredictionTick,
    /// Whether this is the forward pass or reconciliation re-simulation.
    pub pass: PredictionPass,
}

/// Shared read-only execution context that prediction systems may capture.
///
/// Systems should suppress network sends, audio, particles, and other
/// externally visible side effects when [`Self::current`] reports a
/// re-simulation pass.
#[derive(Debug, Clone)]
pub struct PredictionExecutionContext {
    state: Arc<ExecutionState>,
}

impl PredictionExecutionContext {
    fn new() -> Self {
        Self {
            state: Arc::new(ExecutionState {
                tick: AtomicU64::new(PredictionTick::ZERO.get()),
                pass: AtomicU8::new(FORWARD_PASS),
            }),
        }
    }

    /// Return the tick and pass prepared for the current pipeline invocation.
    #[must_use]
    pub fn current(&self) -> PredictionExecution {
        let pass = match self.state.pass.load(Ordering::Acquire) {
            RESIMULATION_PASS => PredictionPass::Resimulation,
            _ => PredictionPass::Forward,
        };
        PredictionExecution {
            tick: PredictionTick::new(self.state.tick.load(Ordering::Acquire)),
            pass,
        }
    }

    fn set(&self, execution: PredictionExecution) {
        let encoded_pass = match execution.pass {
            PredictionPass::Forward => FORWARD_PASS,
            PredictionPass::Resimulation => RESIMULATION_PASS,
        };
        self.state
            .tick
            .store(execution.tick.get(), Ordering::Release);
        self.state.pass.store(encoded_pass, Ordering::Release);
    }
}

/// Failure while advancing a prediction world.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PredictionError {
    /// A registered ECS system failed.
    #[error(transparent)]
    Run(#[from] RunError),
    /// The monotonic prediction tick exhausted its protocol representation.
    #[error("prediction tick overflow")]
    TickOverflow,
}

/// Dedicated ECS world for fixed-step client prediction.
///
/// Wall-clock pacing, transport, snapshot decoding, history storage, and
/// presentation stay outside this type. Reconciliation restores simulation state and
/// [`Self::restore_tick_for_reconciliation`], then calls [`Self::tick`] in
/// [`PredictionPass::Resimulation`] for each subsequent recorded input.
pub struct PredictionWorld {
    ecs: World,
    pipeline: PredictionPipeline,
    tick_delta: TickDelta,
    current_tick: PredictionTick,
    execution_context: PredictionExecutionContext,
}

impl PredictionWorld {
    /// Create a single-threaded prediction world.
    pub fn new() -> Result<Self, Error> {
        Self::from_ecs(World::new()?)
    }

    /// Turn an existing, independently configured ECS world into a prediction world.
    pub fn from_ecs(mut ecs: World) -> Result<Self, Error> {
        let pipeline = PredictionPipeline::register(&mut ecs)?;
        let execution_context = PredictionExecutionContext::new();
        systems::register(&mut ecs, pipeline, execution_context.clone())?;
        let tick_delta = TickDelta::from_seconds(PREDICTION_TICK_DELTA_SECONDS)?;
        telemetry::describe_metrics();
        Ok(Self {
            ecs,
            pipeline,
            tick_delta,
            current_tick: PredictionTick::ZERO,
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

    /// Return the registered prediction pipeline.
    #[must_use]
    pub const fn pipeline(&self) -> PredictionPipeline {
        self.pipeline
    }

    /// Return the world-bound handle for one prediction phase.
    #[must_use]
    pub const fn phase(&self, phase: PredictionPhase) -> PhaseId {
        self.pipeline.phase(phase)
    }

    /// Return the fixed-step delta used by [`Self::tick`].
    #[must_use]
    pub const fn tick_delta(&self) -> TickDelta {
        self.tick_delta
    }

    /// Return the latest successfully completed prediction tick.
    #[must_use]
    pub const fn current_tick(&self) -> PredictionTick {
        self.current_tick
    }

    /// Return a context handle for systems that need the current tick or pass.
    #[must_use]
    pub fn execution_context(&self) -> PredictionExecutionContext {
        self.execution_context.clone()
    }

    /// Advance the prediction pipeline by exactly one fixed tick.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "blackflower_prediction",
            name = "prediction_tick",
            level = "info",
            skip_all,
            fields(
                tick = tracing::field::Empty,
                pass = telemetry::pass_name(pass),
                delta_seconds = f64::from(self.tick_delta.as_seconds()),
                result = tracing::field::Empty,
                reason = tracing::field::Empty,
            ),
        )
    )]
    pub fn tick(&mut self, pass: PredictionPass) -> Result<bool, PredictionError> {
        let Some(next_tick) = self.current_tick.checked_next() else {
            telemetry::tick_rejected(pass, "tick_overflow");
            return Err(PredictionError::TickOverflow);
        };
        #[cfg(feature = "tracing")]
        tracing::Span::current().record("tick", next_tick.get());
        let previous_execution = self.execution_context.current();
        self.execution_context.set(PredictionExecution {
            tick: next_tick,
            pass,
        });

        let observation = TickObservation::start(pass);
        let run_result = self.ecs.progress(self.tick_delta);
        let result = match run_result {
            Ok(should_continue) => {
                self.current_tick = next_tick;
                Ok(should_continue)
            }
            Err(error) => {
                self.execution_context.set(previous_execution);
                Err(PredictionError::Run(error))
            }
        };
        observation.finish(&result);
        result
    }

    /// Reset the local timeline after simulation state has been restored to `tick`.
    ///
    /// This changes only prediction bookkeeping. The caller must restore every
    /// predicted simulation component to the matching authoritative state before
    /// re-simulating subsequent inputs.
    pub fn restore_tick_for_reconciliation(&mut self, tick: PredictionTick) {
        self.current_tick = tick;
        self.execution_context.set(PredictionExecution {
            tick,
            pass: PredictionPass::Resimulation,
        });
    }
}
