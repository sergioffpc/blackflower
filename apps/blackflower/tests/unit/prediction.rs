use std::error::Error as StdError;

use blackflower_harness::{ClientPrediction, PredictionUpdate};
use blackflower_networking::{ControlFrame, InputSequence, SimulationTick};
use blackflower_networking_protocol::v1::{
    CHARACTER_STATE_COMPONENT_ID, CharacterState, MovementControl,
    OWNER_PREDICTION_STATE_COMPONENT_ID, OwnerPredictionState, TRANSFORM_COMPONENT_ID, Transform,
    VELOCITY_COMPONENT_ID, Velocity, replication_priority,
};
use blackflower_networking_replication::{
    ComponentId, ComponentSampleTick, ComponentState, EntityState, ReplicatedEntityId, Snapshot,
    SnapshotTick,
};
use bytes::Bytes;
use glam::{DVec2, DVec3};

use super::{ClientMovementPrediction, orientation_from_view};

type TestResult<T = ()> = Result<T, Box<dyn StdError>>;

#[test]
fn movement_prediction_converges_within_the_revision_one_tolerances() -> TestResult {
    let mut prediction = ClientMovementPrediction::new()?;
    let bootstrap = movement_snapshot(0, DVec3::ZERO, DVec3::ZERO, None)?;
    assert_eq!(
        prediction.bootstrap(&bootstrap)?,
        PredictionUpdate::Bootstrapped {
            tick: SimulationTick::new(0),
        }
    );

    let control = MovementControl::quantize(DVec2::Y, 0.0, 0.0)?;
    prediction.queue_control(&ControlFrame {
        sequence: InputSequence::new(1),
        execute_tick: SimulationTick::new(1),
        payload: Bytes::copy_from_slice(&control.encode()),
    })?;
    prediction.advance_to(SimulationTick::new(4))?;
    let predicted = prediction
        .predicted_state()
        .ok_or("prediction is missing")?;
    assert!((predicted.position_meters[2] + 5.0 / 60.0).abs() < 0.000_001);

    let authoritative = movement_snapshot(
        4,
        DVec3::new(0.0, 0.0, -0.08),
        DVec3::new(0.0, 0.0, -5.0),
        Some(1),
    )?;
    assert_eq!(
        prediction.apply_snapshot(&authoritative)?,
        PredictionUpdate::Converged {
            tick: SimulationTick::new(4),
        }
    );
    Ok(())
}

#[test]
fn movement_prediction_restores_a_state_outside_tolerance() -> TestResult {
    let mut prediction = ClientMovementPrediction::new()?;
    prediction.bootstrap(&movement_snapshot(0, DVec3::ZERO, DVec3::ZERO, None)?)?;
    prediction.advance_to(SimulationTick::new(4))?;

    let correction = movement_snapshot(4, DVec3::X, DVec3::ZERO, None)?;
    assert_eq!(
        prediction.apply_snapshot(&correction)?,
        PredictionUpdate::Reconciled {
            authoritative_tick: SimulationTick::new(4),
            resimulated_ticks: 0,
        }
    );
    let corrected = prediction
        .predicted_state()
        .ok_or("prediction is missing")?;
    assert!((corrected.position_meters[0] - 1.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn movement_prediction_replays_unacknowledged_ticks_after_a_correction() -> TestResult {
    let mut prediction = ClientMovementPrediction::new()?;
    prediction.bootstrap(&movement_snapshot(0, DVec3::ZERO, DVec3::ZERO, None)?)?;
    let control = MovementControl::quantize(DVec2::Y, 0.0, 0.0)?;
    prediction.queue_control(&ControlFrame {
        sequence: InputSequence::new(1),
        execute_tick: SimulationTick::new(1),
        payload: Bytes::copy_from_slice(&control.encode()),
    })?;
    prediction.advance_to(SimulationTick::new(4))?;

    let correction = movement_snapshot(
        2,
        DVec3::new(1.0, 0.0, -0.04),
        DVec3::new(0.0, 0.0, -5.0),
        Some(1),
    )?;
    assert_eq!(
        prediction.apply_snapshot(&correction)?,
        PredictionUpdate::Reconciled {
            authoritative_tick: SimulationTick::new(2),
            resimulated_ticks: 2,
        }
    );
    let replayed = prediction
        .predicted_state()
        .ok_or("prediction is missing")?;
    assert!((replayed.position_meters[0] - 1.0).abs() < f64::EPSILON);
    assert!((replayed.position_meters[2] + 0.04 + 10.0 / 240.0).abs() < 0.000_001);
    Ok(())
}

fn movement_snapshot(
    tick: u64,
    position: DVec3,
    velocity: DVec3,
    acknowledged: Option<u64>,
) -> TestResult<Snapshot> {
    let sample_tick = ComponentSampleTick::new(tick);
    let transform = Transform::quantize(position, orientation_from_view(DVec2::ZERO))?;
    let components = [
        component(TRANSFORM_COMPONENT_ID, sample_tick, transform.encode())?,
        component(
            VELOCITY_COMPONENT_ID,
            sample_tick,
            Velocity::quantize(velocity)?.encode(),
        )?,
        component(
            CHARACTER_STATE_COMPONENT_ID,
            sample_tick,
            CharacterState::new(true).encode(),
        )?,
        component(
            OWNER_PREDICTION_STATE_COMPONENT_ID,
            sample_tick,
            OwnerPredictionState::new(acknowledged.map(InputSequence::new)).encode(),
        )?,
    ];
    Ok(Snapshot::new(
        SnapshotTick::new(tick),
        [(
            ReplicatedEntityId::try_from_u64(1)?,
            EntityState::new(components)?,
        )],
    )?)
}

fn component(
    id: ComponentId,
    sample_tick: ComponentSampleTick,
    bytes: Bytes,
) -> TestResult<(ComponentId, ComponentState)> {
    let priority = replication_priority(id).ok_or("component priority is missing")?;
    Ok((id, ComponentState::new(sample_tick, priority, bytes)?))
}
