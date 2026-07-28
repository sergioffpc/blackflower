use std::error::Error as StdError;
use std::io;
use std::sync::{Arc, Mutex};

use blackflower_ecs::{Component, Read, TickDelta, World};
use blackflower_simulation::{
    AI_UPDATE_INTERVAL_TICKS, CONTROL_FRAME_INTERVAL_TICKS, INPUT_TIMEOUT_TICKS,
    SIMULATION_TICK_DELTA_SECONDS, SNAPSHOT_INTERVAL_TICKS, SimulationPhase, SimulationPipeline,
    SimulationWorld,
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
            "PrepareSimulationTick",
            "CaptureTickInputs",
            "DeriveActorActions",
            "SolveRigidBodyDynamics",
            "SolvePhysicalPhenomena",
            "SolveAcoustics",
            "DeriveStateTransitions",
            "CommitStateTransitions",
            "UpdateSpatialStructures",
            "SealSimulationTick",
            "UpdateBotPerception",
            "PlanBotTactics",
            "EmitBotControlFrames",
            "PublishTickOutputs",
        ]
    );
    assert_eq!(CONTROL_FRAME_INTERVAL_TICKS, 4);
    assert_eq!(SNAPSHOT_INTERVAL_TICKS, 8);
    assert_eq!(AI_UPDATE_INTERVAL_TICKS, 48);
    assert_eq!(INPUT_TIMEOUT_TICKS, 240);
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
    let prepare_simulation_tick = simulation.phase(SimulationPhase::PrepareSimulationTick);
    simulation
        .ecs_mut()
        .system("RecordSimulationTickDelta", "Probe")?
        .phase(prepare_simulation_tick)?
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
