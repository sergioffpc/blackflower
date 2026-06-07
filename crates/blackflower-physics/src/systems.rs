use blackflower_math::components::Transform;

use crate::components::Velocity;

/// Integrates linear velocity into translation for all entities with both
/// [`Transform`] and [`Velocity`].
///
/// `dt` is the simulation delta time in seconds. For the dedicated server,
/// this is always [`crate::time::TICK_DT_SECS`].
pub fn integrate_movement<'a, I>(iter: I, dt: f32)
where
    I: IntoIterator<Item = (&'a mut Transform, &'a Velocity)>,
{
    for (transform, velocity) in iter {
        transform.translation += velocity.0 * dt;
    }
}
