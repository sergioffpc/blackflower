use blackflower_ecs::{Error, PhaseId, SystemResult, Tag, World};
use strum::IntoStaticStr;

use crate::{PredictionExecutionContext, PredictionPhase, PredictionPipeline, telemetry};

#[derive(Tag)]
struct PredictionSystemDriver;

type SystemCallback = fn(&PredictionExecutionContext) -> SystemResult;

/// A system in the predicted [`PredictionPhase::PrepareTick`] phase.
///
/// These systems open the predicted tick boundary and make state scheduled for
/// that tick visible to prediction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum PrepareTickSystem {
    /// Open the prepared tick and initialize its tick-local working context.
    OpenTick,
    /// Activate accepted commits whose scheduled activation tick is now.
    ActivateScheduledCommits,
}

impl PrepareTickSystem {
    /// Number of systems in `PrepareTick`.
    pub const COUNT: usize = 2;

    /// Stable registration order for `PrepareTick` systems.
    pub const ORDER: [Self; Self::COUNT] = [Self::OpenTick, Self::ActivateScheduledCommits];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::OpenTick => open_tick,
            Self::ActivateScheduledCommits => activate_scheduled_commits,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: PredictionExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PredictionPhase::PrepareTick,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the predicted [`PredictionPhase::CaptureTickInputs`] phase.
///
/// These systems build the immutable, canonical input set consumed by the
/// active prediction tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum CaptureTickInputsSystem {
    /// Capture current or recorded actor control frames for the active pass.
    CaptureActorControlFrames,
}

impl CaptureTickInputsSystem {
    /// Number of systems in `CaptureTickInputs`.
    pub const COUNT: usize = 1;

    /// Stable registration order for `CaptureTickInputs` systems.
    pub const ORDER: [Self; Self::COUNT] = [Self::CaptureActorControlFrames];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::CaptureActorControlFrames => capture_actor_control_frames,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: PredictionExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PredictionPhase::CaptureTickInputs,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the predicted [`PredictionPhase::DeriveActorActions`] phase.
///
/// These systems convert captured actor inputs into deterministic action
/// intents consumed by prediction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum DeriveActorActionsSystem {
    /// Derive deterministic locomotion actions from captured actor inputs.
    DeriveLocomotionActions,
    /// Derive deterministic weapon actions from captured actor inputs.
    DeriveWeaponActions,
    /// Derive deterministic interaction actions from captured actor inputs.
    DeriveInteractionActions,
}

impl DeriveActorActionsSystem {
    /// Number of systems in `DeriveActorActions`.
    pub const COUNT: usize = 3;

    /// Stable registration order for `DeriveActorActions` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::DeriveLocomotionActions,
        Self::DeriveWeaponActions,
        Self::DeriveInteractionActions,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::DeriveLocomotionActions => derive_locomotion_actions,
            Self::DeriveWeaponActions => derive_weapon_actions,
            Self::DeriveInteractionActions => derive_interaction_actions,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: PredictionExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PredictionPhase::DeriveActorActions,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the predicted [`PredictionPhase::SolveRigidBodyDynamics`] phase.
///
/// These systems apply physics commands, advance the predicted rigid-body
/// subset, refresh character support, and capture the resulting facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum SolveRigidBodyDynamicsSystem {
    /// Apply desired velocities and commands to predicted character controllers.
    ApplyCharacterControllerInputs,
    /// Apply velocities, rotations, forces, torques, and impulses to predicted bodies.
    ApplyRigidBodyInputs,
    /// Advance the predicted rigid-body world exactly once at the fixed tick delta.
    AdvanceRigidBodyWorld,
    /// Refresh predicted character support state after the rigid-body step.
    RefreshCharacterGroundState,
    /// Capture predicted rigid-body transforms and velocities.
    CaptureRigidBodyState,
    /// Capture predicted character transforms, velocities, and support state.
    CaptureCharacterState,
    /// Capture canonically ordered contact lifecycle and manifold facts.
    CaptureContactFacts,
}

impl SolveRigidBodyDynamicsSystem {
    /// Number of systems in `SolveRigidBodyDynamics`.
    pub const COUNT: usize = 7;

    /// Stable registration and execution order for rigid-body dynamics systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::ApplyCharacterControllerInputs,
        Self::ApplyRigidBodyInputs,
        Self::AdvanceRigidBodyWorld,
        Self::RefreshCharacterGroundState,
        Self::CaptureRigidBodyState,
        Self::CaptureCharacterState,
        Self::CaptureContactFacts,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::ApplyCharacterControllerInputs => apply_character_controller_inputs,
            Self::ApplyRigidBodyInputs => apply_rigid_body_inputs,
            Self::AdvanceRigidBodyWorld => advance_rigid_body_world,
            Self::RefreshCharacterGroundState => refresh_character_ground_state,
            Self::CaptureRigidBodyState => capture_rigid_body_state,
            Self::CaptureCharacterState => capture_character_state,
            Self::CaptureContactFacts => capture_contact_facts,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: PredictionExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PredictionPhase::SolveRigidBodyDynamics,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the predicted [`PredictionPhase::DeriveStateTransitions`] phase.
///
/// These systems derive canonical candidates for speculative state changes
/// from predicted actor actions and facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum DeriveStateTransitionsSystem {
    /// Derive predicted actor condition and locomotion-state transitions.
    DeriveActorConditionTransitions,
    /// Derive predicted weapon-state transitions.
    DeriveWeaponStateTransitions,
    /// Derive predicted inventory-state transitions.
    DeriveInventoryStateTransitions,
    /// Derive transitions for world objects with prediction enabled.
    DeriveWorldObjectStateTransitions,
    /// Validate, deduplicate, and deterministically order transition candidates.
    CanonicalizeTransitionCandidates,
}

impl DeriveStateTransitionsSystem {
    /// Number of systems in `DeriveStateTransitions`.
    pub const COUNT: usize = 5;

    /// Stable registration and execution order for state-transition systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::DeriveActorConditionTransitions,
        Self::DeriveWeaponStateTransitions,
        Self::DeriveInventoryStateTransitions,
        Self::DeriveWorldObjectStateTransitions,
        Self::CanonicalizeTransitionCandidates,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::DeriveActorConditionTransitions => derive_actor_condition_transitions,
            Self::DeriveWeaponStateTransitions => derive_weapon_state_transitions,
            Self::DeriveInventoryStateTransitions => derive_inventory_state_transitions,
            Self::DeriveWorldObjectStateTransitions => derive_world_object_state_transitions,
            Self::CanonicalizeTransitionCandidates => canonicalize_transition_candidates,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: PredictionExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PredictionPhase::DeriveStateTransitions,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the predicted [`PredictionPhase::CommitStateTransitions`] phase.
///
/// These systems select a valid speculative transition set, build its
/// immutable commit, apply accepted state changes once, and capture the facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum CommitStateTransitionsSystem {
    /// Evaluate transition preconditions against predicted state.
    EvaluateTransitionPreconditions,
    /// Select a deterministic accepted set from compatible transition candidates.
    ResolveTransitionConflicts,
    /// Build an immutable ordered commit for immediate and scheduled transitions.
    BuildTransitionCommit,
    /// Validate aggregate invariants across the complete speculative commit.
    ValidateTransitionCommit,
    /// Apply immediate transitions and schedule accepted future transitions.
    CommitAcceptedTransitions,
    /// Capture canonical facts produced by the committed state changes.
    CaptureCommittedTransitions,
}

impl CommitStateTransitionsSystem {
    /// Number of systems in `CommitStateTransitions`.
    pub const COUNT: usize = 6;

    /// Stable registration and execution order for transition-commit systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::EvaluateTransitionPreconditions,
        Self::ResolveTransitionConflicts,
        Self::BuildTransitionCommit,
        Self::ValidateTransitionCommit,
        Self::CommitAcceptedTransitions,
        Self::CaptureCommittedTransitions,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::EvaluateTransitionPreconditions => evaluate_transition_preconditions,
            Self::ResolveTransitionConflicts => resolve_transition_conflicts,
            Self::BuildTransitionCommit => build_transition_commit,
            Self::ValidateTransitionCommit => validate_transition_commit,
            Self::CommitAcceptedTransitions => commit_accepted_transitions,
            Self::CaptureCommittedTransitions => capture_committed_transitions,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: PredictionExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PredictionPhase::CommitStateTransitions,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the predicted [`PredictionPhase::SealTick`] phase.
///
/// These systems validate and hash the completed predicted state, then make it
/// stable for history, reconciliation, and output consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum SealTickSystem {
    /// Validate invariants across the complete predicted state.
    ValidatePredictedState,
    /// Compute the deterministic hash of canonical predicted state.
    ComputePredictedStateHash,
    /// Make the validated predicted state and hash immutable for this tick.
    SealPredictedState,
}

impl SealTickSystem {
    /// Number of systems in `SealTick`.
    pub const COUNT: usize = 3;

    /// Stable registration and execution order for tick-sealing systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::ValidatePredictedState,
        Self::ComputePredictedStateHash,
        Self::SealPredictedState,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::ValidatePredictedState => validate_predicted_state,
            Self::ComputePredictedStateHash => compute_predicted_state_hash,
            Self::SealPredictedState => seal_predicted_state,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: PredictionExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PredictionPhase::SealTick,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the predicted [`PredictionPhase::SubmitTickOutputs`] phase.
///
/// These systems build an immutable batch from sealed predicted state,
/// suppress duplicate re-simulation effects, and submit it in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum SubmitTickOutputsSystem {
    /// Build the immutable output batch for the completed predicted tick.
    BuildTickOutputBatch,
    /// Remove externally visible effects during re-simulation.
    SuppressResimulationEffects,
    /// Submit the completed batch to in-memory consumers.
    SubmitTickOutputBatch,
}

impl SubmitTickOutputsSystem {
    /// Number of systems in `SubmitTickOutputs`.
    pub const COUNT: usize = 3;

    /// Stable registration and execution order for tick-output systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::BuildTickOutputBatch,
        Self::SuppressResimulationEffects,
        Self::SubmitTickOutputBatch,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::BuildTickOutputBatch => build_tick_output_batch,
            Self::SuppressResimulationEffects => suppress_resimulation_effects,
            Self::SubmitTickOutputBatch => submit_tick_output_batch,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: PredictionExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PredictionPhase::SubmitTickOutputs,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

pub(crate) fn register(
    world: &mut World,
    pipeline: PredictionPipeline,
    execution_context: PredictionExecutionContext,
) -> Result<(), Error> {
    let driver_expression = register_system_driver(world)?;
    let phase = pipeline.phase(PredictionPhase::PrepareTick);
    for system in PrepareTickSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PredictionPhase::CaptureTickInputs);
    for system in CaptureTickInputsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PredictionPhase::DeriveActorActions);
    for system in DeriveActorActionsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PredictionPhase::SolveRigidBodyDynamics);
    for system in SolveRigidBodyDynamicsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PredictionPhase::DeriveStateTransitions);
    for system in DeriveStateTransitionsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PredictionPhase::CommitStateTransitions);
    for system in CommitStateTransitionsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PredictionPhase::SealTick);
    for system in SealTickSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PredictionPhase::SubmitTickOutputs);
    for system in SubmitTickOutputsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }
    Ok(())
}

fn register_system_driver(world: &mut World) -> Result<&'static str, Error> {
    let driver = world.register_tag::<PredictionSystemDriver>()?;
    let driver_entity = world.spawn()?;
    world.add_tag(driver_entity, driver)?;
    Ok(<PredictionSystemDriver as Tag>::NAME)
}

fn open_tick(execution_context: &PredictionExecutionContext) -> SystemResult {
    // Open the tick-local working context prepared by PredictionWorld::tick.
    let _execution = execution_context.current();
    Ok(())
}

fn activate_scheduled_commits(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Make accepted commits scheduled for the active tick visible to prediction.
    Ok(())
}

fn capture_actor_control_frames(execution_context: &PredictionExecutionContext) -> SystemResult {
    // Capture current inputs or select recorded inputs for the active pass.
    let _execution = execution_context.current();
    Ok(())
}

fn derive_locomotion_actions(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Derive deterministic locomotion intents from captured actor inputs.
    Ok(())
}

fn derive_weapon_actions(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Derive deterministic weapon intents from captured actor inputs.
    Ok(())
}

fn derive_interaction_actions(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Derive deterministic interaction intents from captured actor inputs.
    Ok(())
}

fn apply_character_controller_inputs(
    _execution_context: &PredictionExecutionContext,
) -> SystemResult {
    // Apply desired velocities and commands to predicted character controllers.
    Ok(())
}

fn apply_rigid_body_inputs(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Apply velocities, rotations, forces, torques, and impulses to predicted bodies.
    Ok(())
}

fn advance_rigid_body_world(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Advance the predicted rigid-body world exactly once at the fixed tick delta.
    Ok(())
}

fn refresh_character_ground_state(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Refresh predicted character support state after the rigid-body step.
    Ok(())
}

fn capture_rigid_body_state(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Capture predicted rigid-body transforms and velocities.
    Ok(())
}

fn capture_character_state(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Capture predicted character transforms, velocities, and support state.
    Ok(())
}

fn capture_contact_facts(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Capture canonically ordered contact lifecycle and manifold facts.
    Ok(())
}

fn derive_actor_condition_transitions(
    _execution_context: &PredictionExecutionContext,
) -> SystemResult {
    // Derive predicted actor condition and locomotion-state transitions.
    Ok(())
}

fn derive_weapon_state_transitions(
    _execution_context: &PredictionExecutionContext,
) -> SystemResult {
    // Derive predicted weapon-state transitions.
    Ok(())
}

fn derive_inventory_state_transitions(
    _execution_context: &PredictionExecutionContext,
) -> SystemResult {
    // Derive predicted inventory-state transitions.
    Ok(())
}

fn derive_world_object_state_transitions(
    _execution_context: &PredictionExecutionContext,
) -> SystemResult {
    // Derive transitions for world objects with prediction enabled.
    Ok(())
}

fn canonicalize_transition_candidates(
    _execution_context: &PredictionExecutionContext,
) -> SystemResult {
    // Validate, deduplicate, and deterministically order transition candidates.
    Ok(())
}

fn evaluate_transition_preconditions(
    _execution_context: &PredictionExecutionContext,
) -> SystemResult {
    // Evaluate transition preconditions against predicted state.
    Ok(())
}

fn resolve_transition_conflicts(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Select a deterministic accepted set from compatible transition candidates.
    Ok(())
}

fn build_transition_commit(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Build an immutable ordered commit for immediate and scheduled transitions.
    Ok(())
}

fn validate_transition_commit(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Validate aggregate invariants across the complete speculative commit.
    Ok(())
}

fn commit_accepted_transitions(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Apply immediate transitions and schedule accepted future transitions.
    Ok(())
}

fn capture_committed_transitions(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Capture canonical facts produced by the committed state changes.
    Ok(())
}

fn validate_predicted_state(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Validate invariants across the complete predicted state.
    Ok(())
}

fn compute_predicted_state_hash(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Compute the deterministic hash of canonical predicted state.
    Ok(())
}

fn seal_predicted_state(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Make the validated predicted state and hash immutable for this tick.
    Ok(())
}

fn build_tick_output_batch(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Build the immutable output batch for the completed predicted tick.
    Ok(())
}

fn suppress_resimulation_effects(execution_context: &PredictionExecutionContext) -> SystemResult {
    // Remove externally visible effects when the active pass is re-simulation.
    let _execution = execution_context.current();
    Ok(())
}

fn submit_tick_output_batch(_execution_context: &PredictionExecutionContext) -> SystemResult {
    // Submit the completed batch to in-memory consumers.
    Ok(())
}

fn register_system<F>(
    world: &mut World,
    phase: PhaseId,
    driver_expression: &'static str,
    prediction_phase: PredictionPhase,
    system_name: &'static str,
    execution_context: PredictionExecutionContext,
    callback: F,
) -> Result<(), Error>
where
    F: Fn(&PredictionExecutionContext) -> SystemResult + 'static,
{
    world
        .system(system_name, driver_expression)?
        .phase(phase)?
        .project(())?
        .each(move |_context, _entity, ()| {
            callback(&execution_context)?;
            telemetry::system_executed(prediction_phase, system_name, execution_context.current());
            Ok(())
        })?;
    Ok(())
}
