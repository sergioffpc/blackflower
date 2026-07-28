use blackflower_ecs::{BuiltinPhase, Error, PhaseId, World};
use strum::IntoStaticStr;

/// Authoritative simulation ticks executed per second.
pub const SIMULATION_TICK_RATE_HZ: u64 = 240;

/// Human and bot control frames produced per second.
pub const CONTROL_FRAME_RATE_HZ: u64 = 60;

/// Authoritative snapshots produced per second.
pub const SNAPSHOT_RATE_HZ: u64 = 30;

/// Bot perception and tactical updates executed per second.
pub const AI_UPDATE_RATE_HZ: u64 = 5;

/// Simulation ticks covered by one control frame.
pub const CONTROL_FRAME_INTERVAL_TICKS: u64 = SIMULATION_TICK_RATE_HZ / CONTROL_FRAME_RATE_HZ;

/// Simulation ticks between authoritative snapshots.
pub const SNAPSHOT_INTERVAL_TICKS: u64 = SIMULATION_TICK_RATE_HZ / SNAPSHOT_RATE_HZ;

/// Simulation ticks between bot perception and tactical updates.
pub const AI_UPDATE_INTERVAL_TICKS: u64 = SIMULATION_TICK_RATE_HZ / AI_UPDATE_RATE_HZ;

/// Simulation ticks after which missing human input becomes neutral.
pub const INPUT_TIMEOUT_TICKS: u64 = SIMULATION_TICK_RATE_HZ;

const _: () = assert!(SIMULATION_TICK_RATE_HZ.is_multiple_of(CONTROL_FRAME_RATE_HZ));
const _: () = assert!(SIMULATION_TICK_RATE_HZ.is_multiple_of(SNAPSHOT_RATE_HZ));
const _: () = assert!(SIMULATION_TICK_RATE_HZ.is_multiple_of(AI_UPDATE_RATE_HZ));

/// A phase in the authoritative simulation pipeline.
///
/// [`SimulationPhase::ORDER`] is the normative execution order. A phase is a
/// synchronization boundary, not a gameplay subsystem: multiple systems may
/// execute within the same phase when they share its single purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum SimulationPhase {
    /// Prepare the fixed-tick context and activate state scheduled for this tick.
    PrepareTick,
    /// Capture an immutable, canonical set of in-memory inputs for this tick.
    CaptureTickInputs,
    /// Derive actor actions from the captured human, bot, and instructor inputs.
    DeriveActorActions,
    /// Advance characters, rigid bodies, constraints, and collision response.
    SolveRigidBodyDynamics,
    /// Advance ballistics, material response, explosions, fire, and smoke.
    SolvePhysicalPhenomena,
    /// Propagate sound emissions through the active acoustic structure.
    SolveAcoustics,
    /// Derive discrete state transitions from physical and exercise facts.
    DeriveStateTransitions,
    /// Resolve conflicts and apply accepted discrete state transitions once.
    CommitStateTransitions,
    /// Produce versioned collision, navigation, acoustic, and visibility updates.
    UpdateSpatialStructures,
    /// Validate, hash, and make the authoritative tick state immutable.
    SealTick,
    /// Build bot perception from accumulated visual and acoustic observations.
    UpdateBotPerception,
    /// Update bot objectives, navigation paths, and tactical decisions.
    PlanBotTactics,
    /// Convert current bot plans into the same control frames used by humans.
    EmitBotControlFrames,
    /// Submit tick outputs to in-memory consumers.
    SubmitTickOutputs,
}

impl SimulationPhase {
    /// Number of phases in the authoritative simulation pipeline.
    pub const COUNT: usize = 14;

    /// Normative execution order of the authoritative simulation phases.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::PrepareTick,
        Self::CaptureTickInputs,
        Self::DeriveActorActions,
        Self::SolveRigidBodyDynamics,
        Self::SolvePhysicalPhenomena,
        Self::SolveAcoustics,
        Self::DeriveStateTransitions,
        Self::CommitStateTransitions,
        Self::UpdateSpatialStructures,
        Self::SealTick,
        Self::UpdateBotPerception,
        Self::PlanBotTactics,
        Self::EmitBotControlFrames,
        Self::SubmitTickOutputs,
    ];

    /// Stable scheduler entity name for this phase.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }
}

/// World-bound handles for every authoritative simulation phase.
#[derive(Debug, Clone, Copy)]
pub struct SimulationPhases {
    prepare_tick: PhaseId,
    capture_tick_inputs: PhaseId,
    derive_actor_actions: PhaseId,
    solve_rigid_body_dynamics: PhaseId,
    solve_physical_phenomena: PhaseId,
    solve_acoustics: PhaseId,
    derive_state_transitions: PhaseId,
    commit_state_transitions: PhaseId,
    update_spatial_structures: PhaseId,
    seal_tick: PhaseId,
    update_bot_perception: PhaseId,
    plan_bot_tactics: PhaseId,
    emit_bot_control_frames: PhaseId,
    submit_tick_outputs: PhaseId,
}

impl SimulationPhases {
    fn register(world: &mut World) -> Result<Self, Error> {
        let [
            prepare_tick,
            capture_tick_inputs,
            derive_actor_actions,
            solve_rigid_body_dynamics,
            solve_physical_phenomena,
            solve_acoustics,
            derive_state_transitions,
            commit_state_transitions,
            update_spatial_structures,
            seal_tick,
            update_bot_perception,
            plan_bot_tactics,
            emit_bot_control_frames,
            submit_tick_outputs,
        ] = register_phase_chain(world)?;
        Ok(Self {
            prepare_tick,
            capture_tick_inputs,
            derive_actor_actions,
            solve_rigid_body_dynamics,
            solve_physical_phenomena,
            solve_acoustics,
            derive_state_transitions,
            commit_state_transitions,
            update_spatial_structures,
            seal_tick,
            update_bot_perception,
            plan_bot_tactics,
            emit_bot_control_frames,
            submit_tick_outputs,
        })
    }

    /// Return the world-bound handle for one simulation phase.
    #[must_use]
    pub const fn get(self, phase: SimulationPhase) -> PhaseId {
        match phase {
            SimulationPhase::PrepareTick => self.prepare_tick,
            SimulationPhase::CaptureTickInputs => self.capture_tick_inputs,
            SimulationPhase::DeriveActorActions => self.derive_actor_actions,
            SimulationPhase::SolveRigidBodyDynamics => self.solve_rigid_body_dynamics,
            SimulationPhase::SolvePhysicalPhenomena => self.solve_physical_phenomena,
            SimulationPhase::SolveAcoustics => self.solve_acoustics,
            SimulationPhase::DeriveStateTransitions => self.derive_state_transitions,
            SimulationPhase::CommitStateTransitions => self.commit_state_transitions,
            SimulationPhase::UpdateSpatialStructures => self.update_spatial_structures,
            SimulationPhase::SealTick => self.seal_tick,
            SimulationPhase::UpdateBotPerception => self.update_bot_perception,
            SimulationPhase::PlanBotTactics => self.plan_bot_tactics,
            SimulationPhase::EmitBotControlFrames => self.emit_bot_control_frames,
            SimulationPhase::SubmitTickOutputs => self.submit_tick_outputs,
        }
    }
}

/// Registered authoritative simulation phases.
///
/// Register gameplay systems against [`Self::phase`], then advance the
/// dedicated simulation world with [`World::progress`]. The phase-aware
/// scheduler discovers the registered systems and orders them by the dependency
/// chain created here.
#[derive(Debug, Clone, Copy)]
pub struct SimulationPipeline {
    phases: SimulationPhases,
}

impl SimulationPipeline {
    /// Register all authoritative simulation phases in `world`.
    ///
    /// Registration is intended to happen once while constructing a dedicated
    /// authoritative simulation world.
    pub fn register(world: &mut World) -> Result<Self, Error> {
        let phases = SimulationPhases::register(world)?;
        Ok(Self { phases })
    }

    /// Return all world-bound phase handles.
    #[must_use]
    pub const fn phases(self) -> SimulationPhases {
        self.phases
    }

    /// Return the world-bound handle for one simulation phase.
    #[must_use]
    pub const fn phase(self, phase: SimulationPhase) -> PhaseId {
        self.phases.get(phase)
    }
}

fn register_phase_chain(world: &mut World) -> Result<[PhaseId; SimulationPhase::COUNT], Error> {
    let first_phase = SimulationPhase::PrepareTick;
    let first = world.create_phase(
        first_phase.name(),
        Some(world.builtin_phase(BuiltinPhase::OnUpdate)),
    )?;
    let mut registered = [first; SimulationPhase::COUNT];
    let mut previous = first;
    for (slot, phase) in registered
        .iter_mut()
        .skip(1)
        .zip(SimulationPhase::ORDER.into_iter().skip(1))
    {
        let current = create_phase_after(world, phase, previous)?;
        *slot = current;
        previous = current;
    }
    Ok(registered)
}

fn create_phase_after(
    world: &mut World,
    phase: SimulationPhase,
    previous: PhaseId,
) -> Result<PhaseId, Error> {
    world.create_phase(phase.name(), Some(previous))
}
