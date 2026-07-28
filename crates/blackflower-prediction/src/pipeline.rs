use blackflower_ecs::{BuiltinPhase, Error, PhaseId, World};
use strum::IntoStaticStr;

/// A synchronization phase in one predicted fixed-step tick.
///
/// [`PredictionPhase::ORDER`] is the normative execution order. Reconciliation
/// re-enters this same pipeline for re-simulation rather than registering a
/// second ECS pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum PredictionPhase {
    /// Prepare the fixed-tick context and activate state scheduled for this tick.
    PreparePredictionTick,
    /// Capture current input or select the recorded input for a re-simulated tick.
    CapturePredictionInput,
    /// Convert the captured control frame into deterministic local actions.
    DerivePredictedActions,
    /// Advance the subset of motion and physics predicted by the client.
    SolvePredictedDynamics,
    /// Derive speculative discrete transitions from the predicted facts.
    DerivePredictedTransitions,
    /// Apply accepted speculative transitions to predicted state once.
    CommitPredictedTransitions,
    /// Validate and make the completed predicted tick stable for consumers.
    SealPredictionTick,
    /// Publish forward outputs; re-simulation suppresses duplicate external effects.
    PublishPredictionOutputs,
}

impl PredictionPhase {
    /// Number of phases in the prediction pipeline.
    pub const COUNT: usize = 8;

    /// Normative execution order of prediction phases.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::PreparePredictionTick,
        Self::CapturePredictionInput,
        Self::DerivePredictedActions,
        Self::SolvePredictedDynamics,
        Self::DerivePredictedTransitions,
        Self::CommitPredictedTransitions,
        Self::SealPredictionTick,
        Self::PublishPredictionOutputs,
    ];

    /// Stable scheduler entity name for this phase.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }
}

/// World-bound handles for every prediction phase.
#[derive(Debug, Clone, Copy)]
pub struct PredictionPhases {
    prepare_prediction_tick: PhaseId,
    capture_prediction_input: PhaseId,
    derive_predicted_actions: PhaseId,
    solve_predicted_dynamics: PhaseId,
    derive_predicted_transitions: PhaseId,
    commit_predicted_transitions: PhaseId,
    seal_prediction_tick: PhaseId,
    publish_prediction_outputs: PhaseId,
}

impl PredictionPhases {
    fn register(world: &mut World) -> Result<Self, Error> {
        let [
            prepare_prediction_tick,
            capture_prediction_input,
            derive_predicted_actions,
            solve_predicted_dynamics,
            derive_predicted_transitions,
            commit_predicted_transitions,
            seal_prediction_tick,
            publish_prediction_outputs,
        ] = register_phase_chain(world)?;
        Ok(Self {
            prepare_prediction_tick,
            capture_prediction_input,
            derive_predicted_actions,
            solve_predicted_dynamics,
            derive_predicted_transitions,
            commit_predicted_transitions,
            seal_prediction_tick,
            publish_prediction_outputs,
        })
    }

    /// Return the world-bound handle for one prediction phase.
    #[must_use]
    pub const fn get(self, phase: PredictionPhase) -> PhaseId {
        match phase {
            PredictionPhase::PreparePredictionTick => self.prepare_prediction_tick,
            PredictionPhase::CapturePredictionInput => self.capture_prediction_input,
            PredictionPhase::DerivePredictedActions => self.derive_predicted_actions,
            PredictionPhase::SolvePredictedDynamics => self.solve_predicted_dynamics,
            PredictionPhase::DerivePredictedTransitions => self.derive_predicted_transitions,
            PredictionPhase::CommitPredictedTransitions => self.commit_predicted_transitions,
            PredictionPhase::SealPredictionTick => self.seal_prediction_tick,
            PredictionPhase::PublishPredictionOutputs => self.publish_prediction_outputs,
        }
    }
}

/// The single ECS pipeline used by forward prediction and reconciliation.
#[derive(Debug, Clone, Copy)]
pub struct PredictionPipeline {
    phases: PredictionPhases,
}

impl PredictionPipeline {
    /// Register all prediction phases in `world`.
    pub fn register(world: &mut World) -> Result<Self, Error> {
        let phases = PredictionPhases::register(world)?;
        Ok(Self { phases })
    }

    /// Return all world-bound phase handles.
    #[must_use]
    pub const fn phases(self) -> PredictionPhases {
        self.phases
    }

    /// Return the world-bound handle for one prediction phase.
    #[must_use]
    pub const fn phase(self, phase: PredictionPhase) -> PhaseId {
        self.phases.get(phase)
    }
}

fn register_phase_chain(world: &mut World) -> Result<[PhaseId; PredictionPhase::COUNT], Error> {
    let first_phase = PredictionPhase::PreparePredictionTick;
    let first = world.create_phase(
        first_phase.name(),
        Some(world.builtin_phase(BuiltinPhase::OnUpdate)),
    )?;
    let mut registered = [first; PredictionPhase::COUNT];
    let mut previous = first;
    for (slot, phase) in registered
        .iter_mut()
        .skip(1)
        .zip(PredictionPhase::ORDER.into_iter().skip(1))
    {
        let current = create_phase_after(world, phase, previous)?;
        *slot = current;
        previous = current;
    }
    Ok(registered)
}

fn create_phase_after(
    world: &mut World,
    phase: PredictionPhase,
    previous: PhaseId,
) -> Result<PhaseId, Error> {
    world.create_phase(phase.name(), Some(previous))
}
