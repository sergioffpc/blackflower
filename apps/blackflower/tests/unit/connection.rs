use blackflower_networking::SimulationTick;
use blackflower_networking_replication::ReplicatedEntityId;

use super::*;

fn predicted_state() -> Result<PredictedMovementState> {
    Ok(PredictedMovementState {
        controlled_entity: ReplicatedEntityId::try_from_u64(17)?,
        position_meters: [1.0, 2.0, 3.0],
        velocity_meters_per_second: [0.0; 3],
        orientation: [0.0, 0.0, 0.0, 1.0],
        grounded: true,
    })
}

#[test]
fn bridge_selects_the_visual_transition_from_prediction_events() -> Result<()> {
    let predicted = predicted_state()?;
    let ordinary = local_movement_sample(Some(&predicted), &[])?
        .context("ordinary movement sample missing")?;
    assert_eq!(ordinary.kind(), MovementSampleKind::Predicted);

    let bootstrap = ClientEvent::SnapshotApplied {
        tick: SimulationTick::new(1),
        prediction: PredictionUpdate::Bootstrapped {
            tick: SimulationTick::new(1),
        },
    };
    let reset = local_movement_sample(Some(&predicted), &[bootstrap])?
        .context("reset movement sample missing")?;
    assert_eq!(reset.kind(), MovementSampleKind::Reset);

    let event = ClientEvent::SnapshotApplied {
        tick: SimulationTick::new(23),
        prediction: PredictionUpdate::Reconciled {
            authoritative_tick: SimulationTick::new(20),
            resimulated_ticks: 3,
        },
    };
    let reconciled = local_movement_sample(Some(&predicted), &[event])?
        .context("reconciled movement sample missing")?;
    assert_eq!(reconciled.kind(), MovementSampleKind::Reconciled);
    assert_eq!(reconciled.source().get(), predicted.controlled_entity.get());
    assert!(
        reconciled
            .position_meters()
            .into_iter()
            .zip(predicted.position_meters)
            .all(|(sample, prediction)| (sample - prediction).abs() <= f64::EPSILON)
    );
    assert!(
        reconciled
            .orientation()
            .into_iter()
            .zip(predicted.orientation)
            .all(|(sample, prediction)| (sample - prediction).abs() <= f64::EPSILON)
    );
    Ok(())
}

#[test]
fn bridge_clears_movement_when_prediction_is_absent() -> Result<()> {
    assert_eq!(local_movement_sample(None, &[])?, None);
    Ok(())
}
