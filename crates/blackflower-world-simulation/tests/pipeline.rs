use std::error::Error as StdError;
use std::io;
use std::sync::{Arc, Mutex};

use blackflower_ecs::{Component, Read, TickDelta, World};
use blackflower_world_simulation::{
    AcousticMode, CONTROL_FRAME_INTERVAL_TICKS, CaptureTickInputsSystem,
    CommitStateTransitionsSystem, DeriveActorActionsSystem, DeriveStateTransitionsSystem,
    INPUT_FAILSAFE_TICKS, INPUT_GRACE_TICKS, PrepareTickSystem, ResolveHistoricalCommandsSystem,
    SIMULATION_TICK_DELTA_SECONDS, SNAPSHOT_INTERVAL_TICKS, SealTickSystem, SimulationPhase,
    SimulationPipeline, SimulationTick, SimulationWorld, SimulationWorldConfig,
    SolveAcousticsSystem, SolvePhysicalPhenomenaSystem, SolveRigidBodyDynamicsSystem,
    SubmitTickOutputsSystem, UpdateSpatialStructuresSystem,
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
            "ResolveHistoricalCommands",
            "SolveRigidBodyDynamics",
            "SolvePhysicalPhenomena",
            "SolveAcoustics",
            "DeriveStateTransitions",
            "CommitStateTransitions",
            "UpdateSpatialStructures",
            "SealTick",
            "SubmitTickOutputs",
        ]
    );
    assert_eq!(CONTROL_FRAME_INTERVAL_TICKS, 4);
    assert_eq!(SNAPSHOT_INTERVAL_TICKS, 8);
    assert_eq!(INPUT_GRACE_TICKS, 12);
    assert_eq!(INPUT_FAILSAFE_TICKS, 240);
}

#[test]
fn prepare_tick_system_names_are_stable() {
    assert_eq!(
        PrepareTickSystem::ORDER.map(PrepareTickSystem::name),
        [
            "OpenTick",
            "ResetTickTransientStorage",
            "ActivateScheduledCommits",
        ]
    );
}

#[test]
fn capture_tick_inputs_system_names_are_stable() {
    assert_eq!(
        CaptureTickInputsSystem::ORDER.map(CaptureTickInputsSystem::name),
        [
            "CaptureCanonicalActorInputs",
            "CaptureEligibleDiscreteCommands",
        ]
    );
}

#[test]
fn resolve_historical_commands_system_names_are_stable() {
    assert_eq!(
        ResolveHistoricalCommandsSystem::ORDER.map(ResolveHistoricalCommandsSystem::name),
        [
            "ResolveRewindRayCommands",
            "CatchUpLateBallistics",
            "CanonicalizeHistoricalCommandFacts",
        ]
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
            "ApplyQueuedPhenomenonEffects",
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
            "ResolveAssemblyDamage",
            "ResolveFractureAndBondFailures",
            "AdvanceAuthoritativeFireState",
            "AdvanceAuthoritativeSmokeField",
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
            "DeriveDestructionTransitions",
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
            "PublishNavigationChanges",
            "UpdateAcousticStructure",
            "UpdateAuthoritativeVisibilityStructure",
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
            "CanonicalizeSimulationEvents",
            "ComputeAuthoritativeStateHash",
            "SealAuthoritativeState",
        ]
    );
}

#[test]
fn submit_tick_outputs_system_names_are_stable() {
    assert_eq!(
        SubmitTickOutputsSystem::ORDER.map(SubmitTickOutputsSystem::name),
        [
            "BuildTickOutputBatch",
            "BuildCommandDispositionOutput",
            "BuildDueReplicationView",
            "SealTickOutputBatch",
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
fn required_acoustics_rejects_a_tick_without_an_installed_world() -> TestResult {
    let mut simulation = SimulationWorld::new_with_config(SimulationWorldConfig {
        acoustics: AcousticMode::Required,
    })?;
    let Err(error) = simulation.tick() else {
        return Err(io::Error::other("required acoustics accepted a missing runtime").into());
    };
    assert!(
        error
            .to_string()
            .contains("authoritative acoustic world is not installed")
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
