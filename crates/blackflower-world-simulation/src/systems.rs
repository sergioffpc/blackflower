use blackflower_ecs::{Error, PhaseId, SystemResult, Tag, World};
use strum::IntoStaticStr;

use crate::{SimulationExecutionContext, SimulationPhase, SimulationPipeline, telemetry};

#[derive(Tag)]
struct SimulationSystemDriver;

type SystemCallback = fn(&SimulationExecutionContext) -> SystemResult;

/// A system in the authoritative [`SimulationPhase::PrepareTick`] phase.
///
/// These systems open the authoritative tick boundary and make state scheduled
/// for that tick visible to the simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum PrepareTickSystem {
    /// Open the next tick and initialize its tick-local working context.
    OpenTick,
    /// Reset scratch storage left by the previous tick attempt.
    ResetTickTransientStorage,
    /// Activate accepted commits whose scheduled activation tick is now.
    ActivateScheduledCommits,
}

impl PrepareTickSystem {
    /// Number of systems in `PrepareTick`.
    pub const COUNT: usize = 3;

    /// Stable registration order for `PrepareTick` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::OpenTick,
        Self::ResetTickTransientStorage,
        Self::ActivateScheduledCommits,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::OpenTick => open_tick,
            Self::ResetTickTransientStorage => reset_tick_transient_storage,
            Self::ActivateScheduledCommits => activate_scheduled_commits,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::PrepareTick,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::CaptureTickInputs`] phase.
///
/// These systems build the immutable, canonical input set consumed by the
/// active simulation tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum CaptureTickInputsSystem {
    /// Capture the canonical actor input selected for each participant at this tick.
    CaptureCanonicalActorInputs,
    /// Capture network-classified discrete commands eligible for gameplay dispatch.
    CaptureEligibleDiscreteCommands,
}

impl CaptureTickInputsSystem {
    /// Number of systems in `CaptureTickInputs`.
    pub const COUNT: usize = 2;

    /// Stable registration order for `CaptureTickInputs` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::CaptureCanonicalActorInputs,
        Self::CaptureEligibleDiscreteCommands,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::CaptureCanonicalActorInputs => capture_canonical_actor_inputs,
            Self::CaptureEligibleDiscreteCommands => capture_eligible_discrete_commands,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::CaptureTickInputs,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::ResolveHistoricalCommands`] phase.
///
/// These systems resolve bounded historical command classes against immutable
/// retained state and publish canonical current-tick facts. They never mutate
/// historical state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum ResolveHistoricalCommandsSystem {
    /// Resolve hitscan commands against their validated read-only view tick.
    ResolveRewindRayCommands,
    /// Advance late projectiles from their validated historical tick to the current tick.
    CatchUpLateBallistics,
    /// Validate, deduplicate, and deterministically order historical command facts.
    CanonicalizeHistoricalCommandFacts,
}

impl ResolveHistoricalCommandsSystem {
    /// Number of systems in `ResolveHistoricalCommands`.
    pub const COUNT: usize = 3;

    /// Stable registration order for historical-command systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::ResolveRewindRayCommands,
        Self::CatchUpLateBallistics,
        Self::CanonicalizeHistoricalCommandFacts,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::ResolveRewindRayCommands => resolve_rewind_ray_commands,
            Self::CatchUpLateBallistics => catch_up_late_ballistics,
            Self::CanonicalizeHistoricalCommandFacts => canonicalize_historical_command_facts,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::ResolveHistoricalCommands,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::DeriveActorActions`] phase.
///
/// These systems convert captured actor inputs into deterministic action
/// intents consumed by the simulation.
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
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::DeriveActorActions,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::SolveRigidBodyDynamics`] phase.
///
/// These systems apply physics commands, advance the fixed-step world, refresh
/// character support, and capture the resulting authoritative facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum SolveRigidBodyDynamicsSystem {
    /// Apply desired velocities and commands to character controllers.
    ApplyCharacterControllerInputs,
    /// Apply forces, torques, and impulses queued by the previous phenomenon solve.
    ApplyQueuedPhenomenonEffects,
    /// Apply velocities, rotations, forces, torques, and impulses to rigid bodies.
    ApplyRigidBodyInputs,
    /// Advance the rigid-body world exactly once at the fixed tick delta.
    AdvanceRigidBodyWorld,
    /// Refresh character support state from the completed rigid-body step.
    RefreshCharacterGroundState,
    /// Capture authoritative rigid-body transforms and velocities.
    CaptureRigidBodyState,
    /// Capture authoritative character transforms, velocities, and support state.
    CaptureCharacterState,
    /// Capture canonically ordered contact lifecycle and manifold facts.
    CaptureContactFacts,
}

impl SolveRigidBodyDynamicsSystem {
    /// Number of systems in `SolveRigidBodyDynamics`.
    pub const COUNT: usize = 8;

    /// Stable registration and execution order for rigid-body dynamics systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::ApplyCharacterControllerInputs,
        Self::ApplyQueuedPhenomenonEffects,
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
            Self::ApplyQueuedPhenomenonEffects => apply_queued_phenomenon_effects,
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
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::SolveRigidBodyDynamics,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::SolvePhysicalPhenomena`] phase.
///
/// These systems advance active physical phenomena, resolve their interactions,
/// queue their rigid-body effects, and capture the resulting authoritative facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum SolvePhysicalPhenomenaSystem {
    /// Advance projectile and fragment trajectories and produce impact candidates.
    AdvanceBallistics,
    /// Resolve blast, fragmentation, pressure, and thermal effects.
    ResolveExplosions,
    /// Resolve penetration, ricochet, fracture, deformation, and ignition candidates.
    ResolveMaterialResponses,
    /// Resolve damage accumulated by layered assemblies and structural members.
    ResolveAssemblyDamage,
    /// Resolve deterministic bond, chunk, and fracture failures.
    ResolveFractureAndBondFailures,
    /// Advance coarse authoritative combustion, fuel, heat, and extinction state.
    AdvanceAuthoritativeFireState,
    /// Advance coarse authoritative smoke fields used by gameplay and replication.
    AdvanceAuthoritativeSmokeField,
    /// Queue forces, torques, and impulses for the next rigid-body step.
    QueueRigidBodyEffects,
    /// Capture the canonical physical facts consumed by later phases.
    CapturePhenomenonFacts,
}

impl SolvePhysicalPhenomenaSystem {
    /// Number of systems in `SolvePhysicalPhenomena`.
    pub const COUNT: usize = 9;

    /// Stable registration and execution order for physical-phenomena systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::AdvanceBallistics,
        Self::ResolveExplosions,
        Self::ResolveMaterialResponses,
        Self::ResolveAssemblyDamage,
        Self::ResolveFractureAndBondFailures,
        Self::AdvanceAuthoritativeFireState,
        Self::AdvanceAuthoritativeSmokeField,
        Self::QueueRigidBodyEffects,
        Self::CapturePhenomenonFacts,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::AdvanceBallistics => advance_ballistics,
            Self::ResolveExplosions => resolve_explosions,
            Self::ResolveMaterialResponses => resolve_material_responses,
            Self::ResolveAssemblyDamage => resolve_assembly_damage,
            Self::ResolveFractureAndBondFailures => resolve_fracture_and_bond_failures,
            Self::AdvanceAuthoritativeFireState => advance_authoritative_fire_state,
            Self::AdvanceAuthoritativeSmokeField => advance_authoritative_smoke_field,
            Self::QueueRigidBodyEffects => queue_rigid_body_effects,
            Self::CapturePhenomenonFacts => capture_phenomenon_facts,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::SolvePhysicalPhenomena,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::SolveAcoustics`] phase.
///
/// These systems capture sound emissions, propagate them through the active
/// acoustic structure, and produce authoritative receiver observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum SolveAcousticsSystem {
    /// Capture and canonicalize sound emissions produced during the active tick.
    CaptureSoundEmissions,
    /// Resolve direct, occluded, transmitted, diffracted, and reflected paths.
    ResolveAcousticPaths,
    /// Advance acoustic energy, attenuation, and time of arrival along resolved paths.
    AdvanceAcousticPropagation,
    /// Build audibility observations for actors, bots, and acoustic sensors.
    BuildAcousticObservations,
    /// Capture the canonical acoustic facts consumed by later phases.
    CaptureAcousticFacts,
}

impl SolveAcousticsSystem {
    /// Number of systems in `SolveAcoustics`.
    pub const COUNT: usize = 5;

    /// Stable registration and execution order for acoustic systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::CaptureSoundEmissions,
        Self::ResolveAcousticPaths,
        Self::AdvanceAcousticPropagation,
        Self::BuildAcousticObservations,
        Self::CaptureAcousticFacts,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::CaptureSoundEmissions => capture_sound_emissions,
            Self::ResolveAcousticPaths => resolve_acoustic_paths,
            Self::AdvanceAcousticPropagation => advance_acoustic_propagation,
            Self::BuildAcousticObservations => build_acoustic_observations,
            Self::CaptureAcousticFacts => capture_acoustic_facts,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::SolveAcoustics,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::DeriveStateTransitions`] phase.
///
/// These systems derive canonical candidates for discrete state changes from
/// actor actions and the facts produced during the active tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum DeriveStateTransitionsSystem {
    /// Derive actor condition and locomotion-state transition candidates.
    DeriveActorConditionTransitions,
    /// Derive weapon-state transition candidates from weapon actions and physical facts.
    DeriveWeaponStateTransitions,
    /// Derive inventory-state transition candidates from interaction actions.
    DeriveInventoryStateTransitions,
    /// Derive interactive and destructible world-object transition candidates.
    DeriveWorldObjectStateTransitions,
    /// Derive structural destruction and bond/chunk transition candidates.
    DeriveDestructionTransitions,
    /// Derive lifecycle transition candidates for active physical phenomena.
    DerivePhenomenonLifecycleTransitions,
    /// Validate, deduplicate, and deterministically order transition candidates.
    CanonicalizeTransitionCandidates,
}

impl DeriveStateTransitionsSystem {
    /// Number of systems in `DeriveStateTransitions`.
    pub const COUNT: usize = 7;

    /// Stable registration and execution order for state-transition systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::DeriveActorConditionTransitions,
        Self::DeriveWeaponStateTransitions,
        Self::DeriveInventoryStateTransitions,
        Self::DeriveWorldObjectStateTransitions,
        Self::DeriveDestructionTransitions,
        Self::DerivePhenomenonLifecycleTransitions,
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
            Self::DeriveDestructionTransitions => derive_destruction_transitions,
            Self::DerivePhenomenonLifecycleTransitions => derive_phenomenon_lifecycle_transitions,
            Self::CanonicalizeTransitionCandidates => canonicalize_transition_candidates,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::DeriveStateTransitions,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::CommitStateTransitions`] phase.
///
/// These systems select a valid transition set, build its immutable commit,
/// apply accepted state changes once, and capture the committed facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum CommitStateTransitionsSystem {
    /// Evaluate transition preconditions against the authoritative state.
    EvaluateTransitionPreconditions,
    /// Select a deterministic accepted set from compatible transition candidates.
    ResolveTransitionConflicts,
    /// Build an immutable ordered commit for immediate and scheduled transitions.
    BuildTransitionCommit,
    /// Validate aggregate state invariants across the complete transition commit.
    ValidateTransitionCommit,
    /// Atomically apply immediate transitions and schedule accepted future transitions.
    CommitAcceptedTransitions,
    /// Capture the canonical facts produced by the committed state changes.
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
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::CommitStateTransitions,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::UpdateSpatialStructures`] phase.
///
/// These systems derive affected regions, update each authoritative spatial
/// structure, publish their versions together, and capture the resulting facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum UpdateSpatialStructuresSystem {
    /// Derive canonical dirty regions and structure changes from committed transitions.
    DeriveSpatialStructureChanges,
    /// Update collision bodies, shapes, filters, and broad-phase state.
    UpdateCollisionStructure,
    /// Publish traversability changes for external client and bot navigation runtimes.
    PublishNavigationChanges,
    /// Update acoustic geometry, materials, and propagation connectivity.
    UpdateAcousticStructure,
    /// Update authoritative gameplay line-of-sight acceleration data.
    UpdateAuthoritativeVisibilityStructure,
    /// Atomically publish the complete set of updated spatial structure versions.
    PublishSpatialStructureVersions,
    /// Capture canonical facts describing changed structures, regions, and versions.
    CaptureSpatialStructureFacts,
}

impl UpdateSpatialStructuresSystem {
    /// Number of systems in `UpdateSpatialStructures`.
    pub const COUNT: usize = 7;

    /// Stable registration and execution order for spatial-structure systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::DeriveSpatialStructureChanges,
        Self::UpdateCollisionStructure,
        Self::PublishNavigationChanges,
        Self::UpdateAcousticStructure,
        Self::UpdateAuthoritativeVisibilityStructure,
        Self::PublishSpatialStructureVersions,
        Self::CaptureSpatialStructureFacts,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::DeriveSpatialStructureChanges => derive_spatial_structure_changes,
            Self::UpdateCollisionStructure => update_collision_structure,
            Self::PublishNavigationChanges => publish_navigation_changes,
            Self::UpdateAcousticStructure => update_acoustic_structure,
            Self::UpdateAuthoritativeVisibilityStructure => {
                update_authoritative_visibility_structure
            }
            Self::PublishSpatialStructureVersions => publish_spatial_structure_versions,
            Self::CaptureSpatialStructureFacts => capture_spatial_structure_facts,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::UpdateSpatialStructures,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::SealTick`] phase.
///
/// These systems validate the final authoritative state, compute its
/// deterministic identity, and make the completed tick immutable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum SealTickSystem {
    /// Validate invariants across the complete authoritative state.
    ValidateAuthoritativeState,
    /// Assign stable identities and canonical order to simulation events.
    CanonicalizeSimulationEvents,
    /// Compute the deterministic hash of the canonical authoritative state.
    ComputeAuthoritativeStateHash,
    /// Make the validated state and its hash immutable for the completed tick.
    SealAuthoritativeState,
}

impl SealTickSystem {
    /// Number of systems in `SealTick`.
    pub const COUNT: usize = 4;

    /// Stable registration and execution order for tick-sealing systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::ValidateAuthoritativeState,
        Self::CanonicalizeSimulationEvents,
        Self::ComputeAuthoritativeStateHash,
        Self::SealAuthoritativeState,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::ValidateAuthoritativeState => validate_authoritative_state,
            Self::CanonicalizeSimulationEvents => canonicalize_simulation_events,
            Self::ComputeAuthoritativeStateHash => compute_authoritative_state_hash,
            Self::SealAuthoritativeState => seal_authoritative_state,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::SealTick,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::SubmitTickOutputs`] phase.
///
/// These systems build a batch from sealed state, attach a transport-neutral
/// replication view when due, and submit it to in-memory consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum SubmitTickOutputsSystem {
    /// Build the immutable output batch for the completed tick.
    BuildTickOutputBatch,
    /// Add final gameplay-owned dispositions for discrete commands.
    BuildCommandDispositionOutput,
    /// Add a transport-neutral replication source view when its cadence is due.
    BuildDueReplicationView,
    /// Seal the tick-keyed output batch for idempotent fan-out.
    SealTickOutputBatch,
    /// Submit the completed batch to in-memory consumers.
    SubmitTickOutputBatch,
}

impl SubmitTickOutputsSystem {
    /// Number of systems in `SubmitTickOutputs`.
    pub const COUNT: usize = 5;

    /// Stable registration and execution order for tick-output systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::BuildTickOutputBatch,
        Self::BuildCommandDispositionOutput,
        Self::BuildDueReplicationView,
        Self::SealTickOutputBatch,
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
            Self::BuildCommandDispositionOutput => build_command_disposition_output,
            Self::BuildDueReplicationView => build_due_replication_view,
            Self::SealTickOutputBatch => seal_tick_output_batch,
            Self::SubmitTickOutputBatch => submit_tick_output_batch,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: SimulationExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            SimulationPhase::SubmitTickOutputs,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

pub(crate) fn register(
    world: &mut World,
    pipeline: SimulationPipeline,
    execution_context: SimulationExecutionContext,
) -> Result<(), Error> {
    let driver_expression = register_system_driver(world)?;
    let phase = pipeline.phase(SimulationPhase::PrepareTick);
    for system in PrepareTickSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(SimulationPhase::CaptureTickInputs);
    for system in CaptureTickInputsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(SimulationPhase::DeriveActorActions);
    for system in DeriveActorActionsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(SimulationPhase::ResolveHistoricalCommands);
    for system in ResolveHistoricalCommandsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(SimulationPhase::SolveRigidBodyDynamics);
    for system in SolveRigidBodyDynamicsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(SimulationPhase::SolvePhysicalPhenomena);
    for system in SolvePhysicalPhenomenaSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(SimulationPhase::SolveAcoustics);
    for system in SolveAcousticsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    register_state_and_spatial_systems(
        world,
        pipeline,
        driver_expression,
        execution_context.clone(),
    )?;
    register_post_seal_systems(world, pipeline, driver_expression, execution_context)?;
    Ok(())
}

fn register_state_and_spatial_systems(
    world: &mut World,
    pipeline: SimulationPipeline,
    driver_expression: &'static str,
    execution_context: SimulationExecutionContext,
) -> Result<(), Error> {
    let phase = pipeline.phase(SimulationPhase::DeriveStateTransitions);
    for system in DeriveStateTransitionsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(SimulationPhase::CommitStateTransitions);
    for system in CommitStateTransitionsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(SimulationPhase::UpdateSpatialStructures);
    for system in UpdateSpatialStructuresSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }
    Ok(())
}

fn register_post_seal_systems(
    world: &mut World,
    pipeline: SimulationPipeline,
    driver_expression: &'static str,
    execution_context: SimulationExecutionContext,
) -> Result<(), Error> {
    let phase = pipeline.phase(SimulationPhase::SealTick);
    for system in SealTickSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(SimulationPhase::SubmitTickOutputs);
    for system in SubmitTickOutputsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }
    Ok(())
}

fn register_system_driver(world: &mut World) -> Result<&'static str, Error> {
    let driver = world.register_tag::<SimulationSystemDriver>()?;
    let driver_entity = world.spawn()?;
    world.add_tag(driver_entity, driver)?;
    Ok(<SimulationSystemDriver as Tag>::NAME)
}

fn open_tick(execution_context: &SimulationExecutionContext) -> SystemResult {
    execution_context.open_next()?;
    Ok(())
}

fn reset_tick_transient_storage(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Clear tick-local candidates, facts, command results, and output staging.
    Ok(())
}

fn activate_scheduled_commits(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Make accepted commits scheduled for the active tick visible to the simulation.
    Ok(())
}

fn capture_canonical_actor_inputs(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Capture the already selected hold, neutral, or canonical actor input for this tick.
    Ok(())
}

fn capture_eligible_discrete_commands(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Capture network-classified commands without importing transport policy.
    Ok(())
}

fn derive_locomotion_actions(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Derive deterministic locomotion intents from captured actor inputs.
    Ok(())
}

fn derive_weapon_actions(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Derive deterministic weapon intents from captured actor inputs.
    Ok(())
}

fn derive_interaction_actions(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Derive deterministic interaction intents from captured actor inputs.
    Ok(())
}

fn resolve_rewind_ray_commands(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Query the validated immutable historical view and emit current-tick hit facts.
    Ok(())
}

fn catch_up_late_ballistics(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Advance a late projectile through the bounded retained history into this tick.
    Ok(())
}

fn canonicalize_historical_command_facts(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Validate, deduplicate, and deterministically order historical command results.
    Ok(())
}

fn apply_character_controller_inputs(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Apply desired velocities and commands to character controllers.
    Ok(())
}

fn apply_queued_phenomenon_effects(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Apply forces, torques, and impulses queued by the previous tick's phenomena.
    Ok(())
}

fn apply_rigid_body_inputs(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Apply velocities, rotations, forces, torques, and impulses to rigid bodies.
    Ok(())
}

fn advance_rigid_body_world(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Advance the rigid-body world exactly once using the authoritative tick delta.
    Ok(())
}

fn refresh_character_ground_state(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Refresh character support state from the completed rigid-body step.
    Ok(())
}

fn capture_rigid_body_state(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Capture authoritative rigid-body transforms and velocities.
    Ok(())
}

fn capture_character_state(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Capture authoritative character transforms, velocities, and support state.
    Ok(())
}

fn capture_contact_facts(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Capture canonically ordered contact lifecycle and manifold facts.
    Ok(())
}

fn advance_ballistics(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Advance projectile and fragment trajectories and produce impact candidates.
    Ok(())
}

fn resolve_explosions(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Resolve blast, fragmentation, pressure, and thermal effects.
    Ok(())
}

fn resolve_material_responses(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Resolve penetration, ricochet, fracture, deformation, and ignition candidates.
    Ok(())
}

fn resolve_assembly_damage(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Accumulate deterministic damage across authored assembly layers and members.
    Ok(())
}

fn resolve_fracture_and_bond_failures(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Resolve authoritative fracture, chunk, and bond failures without presentation voxels.
    Ok(())
}

fn advance_authoritative_fire_state(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Advance coarse combustion, fuel, heat, ignition, and extinction state.
    Ok(())
}

fn advance_authoritative_smoke_field(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Advance bounded gameplay smoke fields; high-resolution voxels remain presentation-only.
    Ok(())
}

fn queue_rigid_body_effects(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Queue forces, torques, and impulses for the next rigid-body step.
    Ok(())
}

fn capture_phenomenon_facts(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Capture the canonical physical facts consumed by later phases.
    Ok(())
}

fn capture_sound_emissions(execution_context: &SimulationExecutionContext) -> SystemResult {
    execution_context.capture_acoustic_tick()?;
    Ok(())
}

fn resolve_acoustic_paths(execution_context: &SimulationExecutionContext) -> SystemResult {
    execution_context.resolve_acoustic_paths()?;
    Ok(())
}

fn advance_acoustic_propagation(execution_context: &SimulationExecutionContext) -> SystemResult {
    execution_context.advance_acoustic_propagation()?;
    Ok(())
}

fn build_acoustic_observations(execution_context: &SimulationExecutionContext) -> SystemResult {
    execution_context.build_acoustic_observations()?;
    Ok(())
}

fn capture_acoustic_facts(execution_context: &SimulationExecutionContext) -> SystemResult {
    execution_context.capture_acoustic_facts()?;
    Ok(())
}

fn derive_actor_condition_transitions(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Derive actor condition and locomotion-state transition candidates.
    Ok(())
}

fn derive_weapon_state_transitions(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Derive weapon-state transition candidates from weapon actions and physical facts.
    Ok(())
}

fn derive_inventory_state_transitions(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Derive inventory-state transition candidates from interaction actions.
    Ok(())
}

fn derive_world_object_state_transitions(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Derive interactive and destructible world-object transition candidates.
    Ok(())
}

fn derive_destruction_transitions(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Derive structural bond, chunk, breach, and destruction transition candidates.
    Ok(())
}

fn derive_phenomenon_lifecycle_transitions(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Derive lifecycle transition candidates for active physical phenomena.
    Ok(())
}

fn canonicalize_transition_candidates(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Validate, deduplicate, and deterministically order transition candidates.
    Ok(())
}

fn evaluate_transition_preconditions(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Evaluate transition preconditions against the authoritative state.
    Ok(())
}

fn resolve_transition_conflicts(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Select a deterministic accepted set from compatible transition candidates.
    Ok(())
}

fn build_transition_commit(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Build an immutable ordered commit for immediate and scheduled transitions.
    Ok(())
}

fn validate_transition_commit(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Validate aggregate state invariants across the complete transition commit.
    Ok(())
}

fn commit_accepted_transitions(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Atomically apply immediate transitions and schedule accepted future transitions.
    Ok(())
}

fn capture_committed_transitions(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Capture the canonical facts produced by the committed state changes.
    Ok(())
}

fn derive_spatial_structure_changes(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Derive canonical dirty regions and structure changes from committed transitions.
    Ok(())
}

fn update_collision_structure(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Update collision bodies, shapes, filters, and broad-phase state.
    Ok(())
}

fn publish_navigation_changes(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Publish traversability changes; external clients and bots own Detour runtime updates.
    Ok(())
}

fn update_acoustic_structure(execution_context: &SimulationExecutionContext) -> SystemResult {
    execution_context.update_acoustic_structure()?;
    Ok(())
}

fn update_authoritative_visibility_structure(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Update only gameplay-authoritative line-of-sight acceleration data.
    Ok(())
}

fn publish_spatial_structure_versions(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Atomically publish the complete set of updated spatial structure versions.
    Ok(())
}

fn capture_spatial_structure_facts(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Capture canonical facts describing changed structures, regions, and versions.
    Ok(())
}

fn validate_authoritative_state(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Validate invariants across the complete authoritative state.
    Ok(())
}

fn canonicalize_simulation_events(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Assign replay-stable identities and canonical order before outputs are built.
    Ok(())
}

fn compute_authoritative_state_hash(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Compute the deterministic hash of the canonical authoritative state.
    Ok(())
}

fn seal_authoritative_state(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Make the validated state and its hash immutable for the completed tick.
    Ok(())
}

fn build_tick_output_batch(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Build the immutable output batch for the completed tick.
    Ok(())
}

fn build_command_disposition_output(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Add gameplay-owned committed, rejected, and superseded command results.
    Ok(())
}

fn build_due_replication_view(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Add a sealed transport-neutral state view for the replication layer.
    Ok(())
}

fn seal_tick_output_batch(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Seal the tick-keyed batch so retries and multiple consumers remain idempotent.
    Ok(())
}

fn submit_tick_output_batch(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Submit the completed batch to in-memory consumers.
    Ok(())
}

fn register_system<F>(
    world: &mut World,
    phase: PhaseId,
    driver_expression: &'static str,
    simulation_phase: SimulationPhase,
    system_name: &'static str,
    execution_context: SimulationExecutionContext,
    callback: F,
) -> Result<(), Error>
where
    F: Fn(&SimulationExecutionContext) -> SystemResult + 'static,
{
    world
        .system(system_name, driver_expression)?
        .phase(phase)?
        .project(())?
        .each(move |_context, _entity, ()| {
            callback(&execution_context)?;
            telemetry::system_executed(
                simulation_phase,
                system_name,
                execution_context.current().tick,
            );
            Ok(())
        })?;
    Ok(())
}
