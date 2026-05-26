//! Simulation systems.
//!
//! Systems are plain functions that operate on the [`World`]. They have no
//! shared trait — the convention is `(world: &mut World, ...inputs)`. The
//! engine invokes them from the tick loop in a deterministic order.

use hecs::World;

use crate::ecs::components::{Transform, Velocity};

/// Integrates linear velocity into translation for all entities with both
/// [`Transform`] and [`Velocity`].
///
/// `dt` is the simulation delta time in seconds. For the dedicated server,
/// this is always [`crate::time::TICK_DT_SECS`].
pub fn integrate_movement(world: &mut World, dt: f32) {
    for (transform, velocity) in world.query_mut::<(&mut Transform, &Velocity)>() {
        transform.translation += velocity.0 * dt;
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs::components::{Transform, Velocity};

    use super::*;

    /// One second of simulation at unit velocity advances translation by `velocity`.
    #[test]
    fn integrate_one_second_advances_by_velocity() {
        let mut world = World::new();
        let entity = world.spawn((
            Transform::identity(),
            Velocity(glam::Vec3::new(1.0, 0.0, 0.0)),
        ));

        // 60 ticks at 1/60s each = 1 second.
        for _ in 0..60 {
            integrate_movement(&mut world, 1.0 / 60.0);
        }

        #[allow(clippy::unwrap_used)]
        let transform = world.get::<&Transform>(entity).unwrap();
        let diff = transform.translation - glam::Vec3::new(1.0, 0.0, 0.0);
        // Allow tiny floating-point drift over 60 sub-steps.
        assert!(
            diff.length() < 1e-5,
            "expected ~Vec3(1,0,0), got {:?}",
            transform.translation
        );
    }

    /// Entities without `Velocity` are not moved.
    #[test]
    fn entity_without_velocity_stays_put() {
        let mut world = World::new();
        let entity = world.spawn((Transform::identity(),));

        integrate_movement(&mut world, 1.0);

        #[allow(clippy::unwrap_used)]
        let transform = world.get::<&Transform>(entity).unwrap();
        assert_eq!(transform.translation, glam::Vec3::ZERO);
    }
}
