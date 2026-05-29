use crate::ecs::{
    SimulationWorld,
    components::{Transform, Velocity},
};

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
