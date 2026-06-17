use blackflower_math::Vec3;
use rapier3d::control::KinematicCharacterController;
use rapier3d::prelude::*;

/// Server-authoritative collision world.
///
/// Static arena solids as cuboid colliders, plus a kinematic character
/// controller for player move-and-slide.
///
/// This runs only on the server. The client does not predict collision (it
/// applies the pure movement system and is corrected by snapshots), so this
/// type is intentionally absent from the predicted path — see ADR 0017.
pub struct CollisionWorld {
    bodies: RigidBodySet,
    colliders: ColliderSet,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    controller: KinematicCharacterController,
}

impl CollisionWorld {
    /// Build from axis-aligned solids given as `(min, max)` world-space corners.
    pub fn from_solids<I>(solids: I) -> Self
    where
        I: IntoIterator<Item = (Vec3, Vec3)>,
    {
        let mut colliders = ColliderSet::new();
        let mut modified = Vec::new();
        for (min, max) in solids {
            let half = (max - min) * 0.5;
            let center = (max + min) * 0.5;
            let collider = ColliderBuilder::cuboid(half.x, half.y, half.z)
                .translation(Vector::new(center.x, center.y, center.z))
                .build();
            modified.push(colliders.insert(collider));
        }

        let bodies = RigidBodySet::new();
        let mut broad_phase = BroadPhaseBvh::new();
        let params = IntegrationParameters::default();
        let mut events = Vec::new();
        broad_phase.update(&params, &colliders, &bodies, &modified, &[], &mut events);

        Self {
            bodies,
            colliders,
            broad_phase,
            narrow_phase: NarrowPhase::new(),
            controller: KinematicCharacterController::default(),
        }
    }

    /// Move a player-sized box from `position` by `displacement`, sliding along
    /// solids. Returns the resolved world position.
    #[must_use]
    pub fn move_and_slide(
        &self,
        position: Vec3,
        half_extents: Vec3,
        displacement: Vec3,
        dt: f32,
    ) -> Vec3 {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            QueryFilter::default().exclude_sensors(),
        );

        let shape = Cuboid::new(Vector::new(half_extents.x, half_extents.y, half_extents.z));
        let mut pose = Pose::IDENTITY;
        pose.translation = Vector::new(position.x, position.y, position.z);

        let movement = self.controller.move_shape(
            dt,
            &query_pipeline,
            &shape,
            &pose,
            Vector::new(displacement.x, displacement.y, displacement.z),
            |_| {},
        );

        position
            + Vec3::new(
                movement.translation.x,
                movement.translation.y,
                movement.translation.z,
            )
    }
}
