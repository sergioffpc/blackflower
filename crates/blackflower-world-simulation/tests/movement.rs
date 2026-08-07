use std::error::Error as StdError;
use std::num::NonZeroU64;

use blackflower_world_simulation::{
    ActorId, INPUT_GRACE_TICKS, MOVEMENT_SPEED_METERS_PER_SECOND, MovementControl,
    SIMULATION_TICK_DELTA_SECONDS, SimulationTick, SimulationWorld,
};

type TestResult = Result<(), Box<dyn StdError>>;

#[test]
fn canonical_control_moves_and_then_neutralizes_one_authoritative_actor() -> TestResult {
    let actor = ActorId::new(NonZeroU64::MIN);
    let mut world = SimulationWorld::new()?;
    world.spawn_movement_actor(actor)?;
    assert!(world.submit_movement_control(MovementControl::new(
        actor,
        7,
        SimulationTick::new(1),
        [0.0, 1.0],
        0.0,
        0.0,
    )?)?);

    for _tick in 0..4 {
        assert!(world.tick()?);
    }
    let moved = *world
        .movement_frame()?
        .actor(actor)
        .ok_or("missing movement actor")?;
    assert_vector_close(moved.velocity_meters_per_second, [0.0, 0.0, -5.0]);
    assert_vector_close(moved.orientation, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(moved.acknowledged_input_sequence, Some(7));
    let expected = -MOVEMENT_SPEED_METERS_PER_SECOND * SIMULATION_TICK_DELTA_SECONDS * 4.0;
    assert!((moved.position_meters[2] - expected).abs() <= f32::EPSILON * 4.0);

    for _tick in 0..INPUT_GRACE_TICKS {
        assert!(world.tick()?);
    }
    let held = *world
        .movement_frame()?
        .actor(actor)
        .ok_or("missing held actor")?;
    assert_vector_close(held.velocity_meters_per_second, [0.0, 0.0, -5.0]);

    for _tick in 0..4 {
        assert!(world.tick()?);
    }
    let neutral = *world
        .movement_frame()?
        .actor(actor)
        .ok_or("missing neutral actor")?;
    assert_vector_close(neutral.velocity_meters_per_second, [0.0; 3]);
    assert_vector_close(neutral.position_meters, held.position_meters);
    assert_eq!(neutral.acknowledged_input_sequence, Some(7));
    Ok(())
}

#[test]
fn stale_control_is_ignored_without_rewinding_state() -> TestResult {
    let actor = ActorId::new(NonZeroU64::MIN);
    let mut world = SimulationWorld::new()?;
    world.spawn_movement_actor(actor)?;
    for _tick in 0..4 {
        assert!(world.tick()?);
    }
    assert!(!world.submit_movement_control(MovementControl::new(
        actor,
        1,
        SimulationTick::new(0),
        [1.0, 0.0],
        0.0,
        0.0,
    )?)?);
    Ok(())
}

fn assert_vector_close<const N: usize>(actual: [f32; N], expected: [f32; N]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() <= f32::EPSILON * 4.0);
    }
}
