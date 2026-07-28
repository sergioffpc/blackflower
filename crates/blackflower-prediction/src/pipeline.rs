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
    PrepareTick,
    /// Capture current tick inputs or select recorded inputs for a re-simulated tick.
    CaptureTickInputs,
    /// Convert the captured control frame into deterministic local actions.
    DeriveActorActions,
    /// Advance the subset of rigid-body dynamics predicted by the client.
    SolveRigidBodyDynamics,
    /// Derive speculative discrete transitions from the predicted facts.
    DeriveStateTransitions,
    /// Apply accepted speculative transitions to predicted state once.
    CommitStateTransitions,
    /// Validate and make the completed predicted tick stable for consumers.
    SealSimulationTick,
    /// Submit forward outputs; re-simulation suppresses duplicate external effects.
    SubmitTickOutputs,
}

impl PredictionPhase {
    /// Number of phases in the prediction pipeline.
    pub const COUNT: usize = 8;

    /// Normative execution order of prediction phases.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::PrepareTick,
        Self::CaptureTickInputs,
        Self::DeriveActorActions,
        Self::SolveRigidBodyDynamics,
        Self::DeriveStateTransitions,
        Self::CommitStateTransitions,
        Self::SealSimulationTick,
        Self::SubmitTickOutputs,
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
    prepare_tick: PhaseId,
    capture_tick_inputs: PhaseId,
    derive_actor_actions: PhaseId,
    solve_rigid_body_dynamics: PhaseId,
    derive_state_transitions: PhaseId,
    commit_state_transitions: PhaseId,
    seal_simulation_tick: PhaseId,
    submit_tick_outputs: PhaseId,
}

impl PredictionPhases {
    fn register(world: &mut World) -> Result<Self, Error> {
        let [
            prepare_tick,
            capture_tick_inputs,
            derive_actor_actions,
            solve_rigid_body_dynamics,
            derive_state_transitions,
            commit_state_transitions,
            seal_simulation_tick,
            submit_tick_outputs,
        ] = register_phase_chain(world)?;
        Ok(Self {
            prepare_tick,
            capture_tick_inputs,
            derive_actor_actions,
            solve_rigid_body_dynamics,
            derive_state_transitions,
            commit_state_transitions,
            seal_simulation_tick,
            submit_tick_outputs,
        })
    }

    /// Return the world-bound handle for one prediction phase.
    #[must_use]
    pub const fn get(self, phase: PredictionPhase) -> PhaseId {
        match phase {
            PredictionPhase::PrepareTick => self.prepare_tick,
            PredictionPhase::CaptureTickInputs => self.capture_tick_inputs,
            PredictionPhase::DeriveActorActions => self.derive_actor_actions,
            PredictionPhase::SolveRigidBodyDynamics => self.solve_rigid_body_dynamics,
            PredictionPhase::DeriveStateTransitions => self.derive_state_transitions,
            PredictionPhase::CommitStateTransitions => self.commit_state_transitions,
            PredictionPhase::SealSimulationTick => self.seal_simulation_tick,
            PredictionPhase::SubmitTickOutputs => self.submit_tick_outputs,
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
    let first_phase = PredictionPhase::PrepareTick;
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
