use std::error::Error as StdError;
use std::io;
use std::sync::{Arc, Mutex};

use blackflower_ecs::{Component, Read, TickDelta, World};
use blackflower_simulation::{
    AI_UPDATE_INTERVAL_TICKS, CONTROL_FRAME_INTERVAL_TICKS, CaptureTickInputsSystem,
    CommitStateTransitionsSystem, DeriveActorActionsSystem, DeriveStateTransitionsSystem,
    EmitBotControlFramesSystem, INPUT_TIMEOUT_TICKS, PlanBotTacticsSystem, PrepareTickSystem,
    SIMULATION_TICK_DELTA_SECONDS, SNAPSHOT_INTERVAL_TICKS, SealTickSystem, SimulationPhase,
    SimulationPipeline, SimulationTick, SimulationWorld, SolveAcousticsSystem,
    SolvePhysicalPhenomenaSystem, SolveRigidBodyDynamicsSystem, SubmitTickOutputsSystem,
    UpdateBotPerceptionSystem, UpdateSpatialStructuresSystem,
};
use bytemuck::{Pod, Zeroable};

type TestResult = Result<(), Box<dyn StdError>>;

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct Probe(u8);

#[test]
fn phase_names_and_scheduling_intervals_are_stable() {
    assert_eq!(
        SimulationPhase::ORDER.map(SimulationPhase::name),
        [
            "PrepareTick",
            "CaptureTickInputs",
            "DeriveActorActions",
            "SolveRigidBodyDynamics",
            "SolvePhysicalPhenomena",
            "SolveAcoustics",
            "DeriveStateTransitions",
            "CommitStateTransitions",
            "UpdateSpatialStructures",
            "SealTick",
            "UpdateBotPerception",
            "PlanBotTactics",
            "EmitBotControlFrames",
            "SubmitTickOutputs",
        ]
    );
    assert_eq!(CONTROL_FRAME_INTERVAL_TICKS, 4);
    assert_eq!(SNAPSHOT_INTERVAL_TICKS, 8);
    assert_eq!(AI_UPDATE_INTERVAL_TICKS, 48);
    assert_eq!(INPUT_TIMEOUT_TICKS, 240);
}

#[test]
fn prepare_tick_system_names_are_stable() {
    assert_eq!(
        PrepareTickSystem::ORDER.map(PrepareTickSystem::name),
        ["OpenTick", "ActivateScheduledCommits"]
    );
}

#[test]
fn capture_tick_inputs_system_names_are_stable() {
    assert_eq!(
        CaptureTickInputsSystem::ORDER.map(CaptureTickInputsSystem::name),
        ["CaptureActorControlFrames"]
    );
}

#[test]
fn derive_actor_actions_system_names_are_stable() {
    assert_eq!(
        DeriveActorActionsSystem::ORDER.map(DeriveActorActionsSystem::name),
        [
            "DeriveLocomotionActions",
            "DeriveWeaponActions",
            "DeriveInteractionActions",
        ]
    );
}

#[test]
fn solve_rigid_body_dynamics_system_names_are_stable() {
    assert_eq!(
        SolveRigidBodyDynamicsSystem::ORDER.map(SolveRigidBodyDynamicsSystem::name),
        [
            "ApplyCharacterControllerInputs",
            "ApplyRigidBodyInputs",
            "AdvanceRigidBodyWorld",
            "RefreshCharacterGroundState",
            "CaptureRigidBodyState",
            "CaptureCharacterState",
            "CaptureContactFacts",
        ]
    );
}

#[test]
fn solve_physical_phenomena_system_names_are_stable() {
    assert_eq!(
        SolvePhysicalPhenomenaSystem::ORDER.map(SolvePhysicalPhenomenaSystem::name),
        [
            "AdvanceBallistics",
            "ResolveExplosions",
            "ResolveMaterialResponses",
            "AdvanceFire",
            "AdvanceSmoke",
            "QueueRigidBodyEffects",
            "CapturePhenomenonFacts",
        ]
    );
}

#[test]
fn solve_acoustics_system_names_are_stable() {
    assert_eq!(
        SolveAcousticsSystem::ORDER.map(SolveAcousticsSystem::name),
        [
            "CaptureSoundEmissions",
            "ResolveAcousticPaths",
            "AdvanceAcousticPropagation",
            "BuildAcousticObservations",
            "CaptureAcousticFacts",
        ]
    );
}

#[test]
fn derive_state_transitions_system_names_are_stable() {
    assert_eq!(
        DeriveStateTransitionsSystem::ORDER.map(DeriveStateTransitionsSystem::name),
        [
            "DeriveActorConditionTransitions",
            "DeriveWeaponStateTransitions",
            "DeriveInventoryStateTransitions",
            "DeriveWorldObjectStateTransitions",
            "DerivePhenomenonLifecycleTransitions",
            "CanonicalizeTransitionCandidates",
        ]
    );
}

#[test]
fn commit_state_transitions_system_names_are_stable() {
    assert_eq!(
        CommitStateTransitionsSystem::ORDER.map(CommitStateTransitionsSystem::name),
        [
            "EvaluateTransitionPreconditions",
            "ResolveTransitionConflicts",
            "BuildTransitionCommit",
            "ValidateTransitionCommit",
            "CommitAcceptedTransitions",
            "CaptureCommittedTransitions",
        ]
    );
}

#[test]
fn update_spatial_structures_system_names_are_stable() {
    assert_eq!(
        UpdateSpatialStructuresSystem::ORDER.map(UpdateSpatialStructuresSystem::name),
        [
            "DeriveSpatialStructureChanges",
            "UpdateCollisionStructure",
            "UpdateNavigationStructure",
            "UpdateAcousticStructure",
            "UpdateVisibilityStructure",
            "PublishSpatialStructureVersions",
            "CaptureSpatialStructureFacts",
        ]
    );
}

#[test]
fn seal_tick_system_names_are_stable() {
    assert_eq!(
        SealTickSystem::ORDER.map(SealTickSystem::name),
        [
            "ValidateAuthoritativeState",
            "ComputeAuthoritativeStateHash",
            "SealAuthoritativeState",
        ]
    );
}

#[test]
fn update_bot_perception_system_names_are_stable() {
    assert_eq!(
        UpdateBotPerceptionSystem::ORDER.map(UpdateBotPerceptionSystem::name),
        [
            "BuildBotVisualObservations",
            "CollectBotAcousticObservations",
            "UpdateBotPerceptionState",
        ]
    );
}

#[test]
fn plan_bot_tactics_system_names_are_stable() {
    assert_eq!(
        PlanBotTacticsSystem::ORDER.map(PlanBotTacticsSystem::name),
        [
            "SelectBotObjectives",
            "BuildBotTacticalPlans",
            "UpdateBotNavigationPaths",
        ]
    );
}

#[test]
fn emit_bot_control_frames_system_names_are_stable() {
    assert_eq!(
        EmitBotControlFramesSystem::ORDER.map(EmitBotControlFramesSystem::name),
        [
            "FollowBotNavigationPaths",
            "BuildBotControlFrames",
            "QueueBotControlFrames",
        ]
    );
}

#[test]
fn submit_tick_outputs_system_names_are_stable() {
    assert_eq!(
        SubmitTickOutputsSystem::ORDER.map(SubmitTickOutputsSystem::name),
        [
            "BuildTickOutputBatch",
            "BuildDueSnapshotOutput",
            "SubmitTickOutputBatch",
        ]
    );
}

#[test]
fn pipeline_orders_systems_by_phase_instead_of_registration_order() -> TestResult {
    let mut world = World::new()?;
    let probe = world.register_component::<Probe>()?;
    let entity = world.spawn()?;
    world.insert(entity, probe, Probe(0))?;

    let pipeline = SimulationPipeline::register(&mut world)?;
    let observed = Arc::new(Mutex::new(Vec::new()));

    for phase in SimulationPhase::ORDER.into_iter().rev() {
        let system_name = format!("Record{}", phase.name());
        let observed_by_system = Arc::clone(&observed);
        world
            .system(&system_name, "Probe")?
            .phase(pipeline.phase(phase))?
            .project(Read::<Probe>::field(0))?
            .each(move |_context, _entity, _probe| {
                let mut observed = observed_by_system
                    .lock()
                    .map_err(|_error| io::Error::other("phase order lock poisoned"))?;
                observed.push(phase);
                Ok(())
            })?;
    }

    assert!(world.progress(TickDelta::from_seconds(1.0 / 240.0)?)?);

    let observed = observed
        .lock()
        .map_err(|_error| io::Error::other("phase order lock poisoned"))?;
    assert_eq!(observed.as_slice(), SimulationPhase::ORDER);
    Ok(())
}

#[test]
fn simulation_world_owns_the_pipeline_and_advances_at_the_fixed_rate() -> TestResult {
    let mut simulation = SimulationWorld::new()?;
    let probe = simulation.ecs_mut().register_component::<Probe>()?;
    let entity = simulation.ecs_mut().spawn()?;
    simulation.ecs_mut().insert(entity, probe, Probe(0))?;

    let observed_delta = Arc::new(Mutex::new(None));
    let observed_delta_by_system = Arc::clone(&observed_delta);
    let prepare_tick = simulation.phase(SimulationPhase::PrepareTick);
    simulation
        .ecs_mut()
        .system("RecordSimulationTickDelta", "Probe")?
        .phase(prepare_tick)?
        .project(Read::<Probe>::field(0))?
        .each(move |context, _entity, _probe| {
            let mut observed_delta = observed_delta_by_system
                .lock()
                .map_err(|_error| io::Error::other("tick delta lock poisoned"))?;
            *observed_delta = Some(context.delta().as_seconds());
            Ok(())
        })?;

    assert_eq!(
        simulation.tick_delta().as_seconds().to_bits(),
        SIMULATION_TICK_DELTA_SECONDS.to_bits()
    );
    assert!(simulation.tick()?);
    assert_eq!(
        observed_delta
            .lock()
            .map_err(|_error| io::Error::other("tick delta lock poisoned"))?
            .as_ref()
            .map(|delta| delta.to_bits()),
        Some(SIMULATION_TICK_DELTA_SECONDS.to_bits())
    );
    Ok(())
}

#[test]
fn open_tick_advances_and_publishes_the_authoritative_tick() -> TestResult {
    let mut simulation = SimulationWorld::new()?;
    let execution_context = simulation.execution_context();
    let probe = simulation.ecs_mut().register_component::<Probe>()?;
    let entity = simulation.ecs_mut().spawn()?;
    simulation.ecs_mut().insert(entity, probe, Probe(0))?;

    let observed_tick = Arc::new(Mutex::new(None));
    let observed_tick_by_system = Arc::clone(&observed_tick);
    let execution_context_by_system = execution_context.clone();
    let capture_inputs = simulation.phase(SimulationPhase::CaptureTickInputs);
    simulation
        .ecs_mut()
        .system("ObserveOpenedTick", "Probe")?
        .phase(capture_inputs)?
        .project(Read::<Probe>::field(0))?
        .each(move |_context, _entity, _probe| {
            let mut observed_tick = observed_tick_by_system
                .lock()
                .map_err(|_error| io::Error::other("opened tick lock poisoned"))?;
            *observed_tick = Some(execution_context_by_system.current().tick);
            Ok(())
        })?;

    assert_eq!(simulation.current_tick(), SimulationTick::ZERO);
    assert_eq!(execution_context.current().tick, SimulationTick::ZERO);

    assert!(simulation.tick()?);
    assert_eq!(simulation.current_tick(), SimulationTick::new(1));
    assert_eq!(execution_context.current().tick, SimulationTick::new(1));
    assert_eq!(
        *observed_tick
            .lock()
            .map_err(|_error| io::Error::other("opened tick lock poisoned"))?,
        Some(SimulationTick::new(1))
    );

    assert!(simulation.tick()?);
    assert_eq!(simulation.current_tick(), SimulationTick::new(2));
    assert_eq!(execution_context.current().tick, SimulationTick::new(2));
    Ok(())
}

#[test]
fn failed_pipeline_execution_does_not_commit_the_opened_tick() -> TestResult {
    let mut simulation = SimulationWorld::new()?;
    let probe = simulation.ecs_mut().register_component::<Probe>()?;
    let entity = simulation.ecs_mut().spawn()?;
    simulation.ecs_mut().insert(entity, probe, Probe(0))?;

    let capture_inputs = simulation.phase(SimulationPhase::CaptureTickInputs);
    simulation
        .ecs_mut()
        .system("FailAfterOpenTick", "Probe")?
        .phase(capture_inputs)?
        .project(Read::<Probe>::field(0))?
        .each(|_context, _entity, _probe| {
            Err(io::Error::other("intentional failure after OpenTick").into())
        })?;

    let error = match simulation.tick() {
        Err(error) => error,
        Ok(_) => return Err(io::Error::other("the system failure must abort the tick").into()),
    };
    assert_eq!(error.system(), Some("FailAfterOpenTick"));
    assert_eq!(simulation.current_tick(), SimulationTick::ZERO);
    assert_eq!(
        simulation.execution_context().current().tick,
        SimulationTick::ZERO
    );
    Ok(())
}
