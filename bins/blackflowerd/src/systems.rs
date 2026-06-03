use blackflower_input::components::InputButtons;
use blackflower_math::{Vec2, Vec3, components::Transform};
use blackflower_physics::components::Velocity;
use blackflower_world::SimulationWorld;

/// Integrates linear velocity into translation for all entities with both
/// [`Transform`] and [`Velocity`].
///
/// `dt` is the simulation delta time in seconds. For the dedicated server,
/// this is always [`crate::time::TICK_DT_SECS`].
pub fn integrate_movement(world: &mut SimulationWorld, dt: f32) {
    for (transform, velocity) in world.query_mut::<(&mut Transform, &Velocity)>() {
        transform.translation += velocity.0 * dt;
    }
}

/// Apply player input to a transform.
///
/// Pure function. Identical results on client and server given identical
/// inputs — this is the prediction/authority consistency requirement.
///
/// Movement is on the XZ plane (Y is up, -Z is forward). Diagonal motion
/// is normalized.
pub fn apply_player_movement(transform: &mut Transform, buttons: InputButtons, dt: f32) {
    let dir = buttons.normalize_or_zero();
    if dir == Vec2::ZERO {
        return;
    }

    /// Player movement speed in world units per second.
    const PLAYER_MOVE_SPEED: f32 = 5.0;
    transform.translation += Vec3::new(dir.x, 0.0, dir.y) * PLAYER_MOVE_SPEED * dt;
}
