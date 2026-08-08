use blackflower_networking::SimulationTick;
use blackflower_networking_replication::ReplicatedEntityId;
use glam::{Quat, Vec3};

use super::*;

fn predicted_state() -> Result<PredictedMovementState> {
    Ok(PredictedMovementState {
        controlled_entity: ReplicatedEntityId::try_from_u64(17)?,
        position_meters: Vec3::new(1.0, 2.0, 3.0),
        velocity_meters_per_second: Vec3::ZERO,
        orientation: Quat::IDENTITY,
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
            .abs_diff_eq(predicted.position_meters, f32::EPSILON)
    );
    assert!(
        reconciled
            .orientation()
            .abs_diff_eq(predicted.orientation, f32::EPSILON)
    );
    Ok(())
}

#[test]
fn bridge_clears_movement_when_prediction_is_absent() -> Result<()> {
    assert_eq!(local_movement_sample(None, &[])?, None);
    Ok(())
}

#[test]
fn resource_registry_keeps_stable_logical_handles() -> Result<()> {
    let mut registry = ClientResourceRegistry::default();
    let player = AssetId::from_str("maps/bootstrap/player")?;
    let other = AssetId::from_str("maps/bootstrap/other")?;
    let player_handle = registry.resolve(&player)?;
    assert_eq!(registry.resolve(&player)?, player_handle);
    assert_ne!(registry.resolve(&other)?, player_handle);
    Ok(())
}
