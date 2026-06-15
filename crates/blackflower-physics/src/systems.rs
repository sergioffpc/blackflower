use blackflower_math::components::Transform;

use crate::components::Velocity;

pub fn integrate_movement<'a, I>(iter: I, dt: f32)
where
    I: IntoIterator<Item = (&'a mut Transform, &'a Velocity)>,
{
    for (transform, velocity) in iter {
        transform.translation += velocity.0 * dt;
    }
}
