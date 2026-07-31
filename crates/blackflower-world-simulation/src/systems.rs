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
    /// Capture the human and bot control frames eligible for the active tick.
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
    /// Advance combustion, fuel consumption, heat propagation, and extinction.
    AdvanceFire,
    /// Advance authoritative smoke emission, transport, expansion, and dissipation.
    AdvanceSmoke,
    /// Queue forces, torques, and impulses for the next rigid-body step.
    QueueRigidBodyEffects,
    /// Capture the canonical physical facts consumed by later phases.
    CapturePhenomenonFacts,
}

impl SolvePhysicalPhenomenaSystem {
    /// Number of systems in `SolvePhysicalPhenomena`.
    pub const COUNT: usize = 7;

    /// Stable registration and execution order for physical-phenomena systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::AdvanceBallistics,
        Self::ResolveExplosions,
        Self::ResolveMaterialResponses,
        Self::AdvanceFire,
        Self::AdvanceSmoke,
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
            Self::AdvanceFire => advance_fire,
            Self::AdvanceSmoke => advance_smoke,
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
    /// Derive lifecycle transition candidates for active physical phenomena.
    DerivePhenomenonLifecycleTransitions,
    /// Validate, deduplicate, and deterministically order transition candidates.
    CanonicalizeTransitionCandidates,
}

impl DeriveStateTransitionsSystem {
    /// Number of systems in `DeriveStateTransitions`.
    pub const COUNT: usize = 6;

    /// Stable registration and execution order for state-transition systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::DeriveActorConditionTransitions,
        Self::DeriveWeaponStateTransitions,
        Self::DeriveInventoryStateTransitions,
        Self::DeriveWorldObjectStateTransitions,
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
    /// Replace affected navigation tiles and update traversability links.
    UpdateNavigationStructure,
    /// Update acoustic geometry, materials, and propagation connectivity.
    UpdateAcousticStructure,
    /// Update visibility occluders and line-of-sight acceleration data.
    UpdateVisibilityStructure,
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
        Self::UpdateNavigationStructure,
        Self::UpdateAcousticStructure,
        Self::UpdateVisibilityStructure,
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
            Self::UpdateNavigationStructure => update_navigation_structure,
            Self::UpdateAcousticStructure => update_acoustic_structure,
            Self::UpdateVisibilityStructure => update_visibility_structure,
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
    /// Compute the deterministic hash of the canonical authoritative state.
    ComputeAuthoritativeStateHash,
    /// Make the validated state and its hash immutable for the completed tick.
    SealAuthoritativeState,
}

impl SealTickSystem {
    /// Number of systems in `SealTick`.
    pub const COUNT: usize = 3;

    /// Stable registration and execution order for tick-sealing systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::ValidateAuthoritativeState,
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

/// A system in the authoritative [`SimulationPhase::UpdateBotPerception`] phase.
///
/// These systems build visual and acoustic observations from sealed state and
/// combine them into the perception state consumed by bot planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum UpdateBotPerceptionSystem {
    /// Build visual observations from sensor limits and line-of-sight queries.
    BuildBotVisualObservations,
    /// Select the acoustic observations received by each bot.
    CollectBotAcousticObservations,
    /// Combine current observations with prior bot perception memory.
    UpdateBotPerceptionState,
}

impl UpdateBotPerceptionSystem {
    /// Number of systems in `UpdateBotPerception`.
    pub const COUNT: usize = 3;

    /// Stable registration and execution order for bot-perception systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::BuildBotVisualObservations,
        Self::CollectBotAcousticObservations,
        Self::UpdateBotPerceptionState,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::BuildBotVisualObservations => build_bot_visual_observations,
            Self::CollectBotAcousticObservations => collect_bot_acoustic_observations,
            Self::UpdateBotPerceptionState => update_bot_perception_state,
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
            SimulationPhase::UpdateBotPerception,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::PlanBotTactics`] phase.
///
/// These systems select bot objectives, build tactical plans, and maintain the
/// navigation paths consumed by bot control-frame generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum PlanBotTacticsSystem {
    /// Select each bot's highest-priority objective from its perception state.
    SelectBotObjectives,
    /// Build concrete tactical plans for the selected objectives.
    BuildBotTacticalPlans,
    /// Calculate or refresh navigation paths for the planned destinations.
    UpdateBotNavigationPaths,
}

impl PlanBotTacticsSystem {
    /// Number of systems in `PlanBotTactics`.
    pub const COUNT: usize = 3;

    /// Stable registration and execution order for bot-tactics systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::SelectBotObjectives,
        Self::BuildBotTacticalPlans,
        Self::UpdateBotNavigationPaths,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::SelectBotObjectives => select_bot_objectives,
            Self::BuildBotTacticalPlans => build_bot_tactical_plans,
            Self::UpdateBotNavigationPaths => update_bot_navigation_paths,
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
            SimulationPhase::PlanBotTactics,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::EmitBotControlFrames`] phase.
///
/// These systems follow planned navigation paths, build canonical actor control
/// frames, and queue them as future in-memory simulation inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum EmitBotControlFramesSystem {
    /// Convert planned navigation paths into current steering controls.
    FollowBotNavigationPaths,
    /// Build canonical actor control frames from steering and tactical plans.
    BuildBotControlFrames,
    /// Queue bot control frames for capture by a future simulation tick.
    QueueBotControlFrames,
}

impl EmitBotControlFramesSystem {
    /// Number of systems in `EmitBotControlFrames`.
    pub const COUNT: usize = 3;

    /// Stable registration and execution order for bot-control-frame systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::FollowBotNavigationPaths,
        Self::BuildBotControlFrames,
        Self::QueueBotControlFrames,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::FollowBotNavigationPaths => follow_bot_navigation_paths,
            Self::BuildBotControlFrames => build_bot_control_frames,
            Self::QueueBotControlFrames => queue_bot_control_frames,
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
            SimulationPhase::EmitBotControlFrames,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the authoritative [`SimulationPhase::SubmitTickOutputs`] phase.
///
/// These systems build a batch from sealed state, attach a snapshot when due,
/// and submit the completed batch to in-memory consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum SubmitTickOutputsSystem {
    /// Build the immutable output batch for the completed tick.
    BuildTickOutputBatch,
    /// Add an authoritative snapshot view when the snapshot cadence is due.
    BuildDueSnapshotOutput,
    /// Submit the completed batch to in-memory consumers.
    SubmitTickOutputBatch,
}

impl SubmitTickOutputsSystem {
    /// Number of systems in `SubmitTickOutputs`.
    pub const COUNT: usize = 3;

    /// Stable registration and execution order for tick-output systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::BuildTickOutputBatch,
        Self::BuildDueSnapshotOutput,
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
            Self::BuildDueSnapshotOutput => build_due_snapshot_output,
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

    let phase = pipeline.phase(SimulationPhase::UpdateBotPerception);
    for system in UpdateBotPerceptionSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(SimulationPhase::PlanBotTactics);
    for system in PlanBotTacticsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(SimulationPhase::EmitBotControlFrames);
    for system in EmitBotControlFramesSystem::ORDER {
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

fn activate_scheduled_commits(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Make accepted commits scheduled for the active tick visible to the simulation.
    Ok(())
}

fn capture_actor_control_frames(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Select and canonicalize the actor control frames eligible for the active tick.
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

fn apply_character_controller_inputs(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Apply desired velocities and commands to character controllers.
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

fn advance_fire(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Advance combustion, fuel consumption, heat propagation, and extinction.
    Ok(())
}

fn advance_smoke(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Advance authoritative smoke emission, transport, expansion, and dissipation.
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

fn update_navigation_structure(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Replace affected navigation tiles and update traversability links.
    Ok(())
}

fn update_acoustic_structure(execution_context: &SimulationExecutionContext) -> SystemResult {
    execution_context.update_acoustic_structure()?;
    Ok(())
}

fn update_visibility_structure(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Update visibility occluders and line-of-sight acceleration data.
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

fn build_bot_visual_observations(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Build visual observations from sensor limits and line-of-sight queries.
    Ok(())
}

fn collect_bot_acoustic_observations(
    _execution_context: &SimulationExecutionContext,
) -> SystemResult {
    // Select the acoustic observations received by each bot.
    Ok(())
}

fn update_bot_perception_state(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Combine current observations with prior bot perception memory.
    Ok(())
}

fn select_bot_objectives(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Select each bot's highest-priority objective from its perception state.
    Ok(())
}

fn build_bot_tactical_plans(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Build concrete tactical plans for the selected objectives.
    Ok(())
}

fn update_bot_navigation_paths(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Calculate or refresh navigation paths for the planned destinations.
    Ok(())
}

fn follow_bot_navigation_paths(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Convert planned navigation paths into current steering controls.
    Ok(())
}

fn build_bot_control_frames(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Build canonical actor control frames from steering and tactical plans.
    Ok(())
}

fn queue_bot_control_frames(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Queue bot control frames for capture by a future simulation tick.
    Ok(())
}

fn build_tick_output_batch(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Build the immutable output batch for the completed tick.
    Ok(())
}

fn build_due_snapshot_output(_execution_context: &SimulationExecutionContext) -> SystemResult {
    // Add an authoritative snapshot view when the snapshot cadence is due.
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
