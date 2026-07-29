use glam::Vec3A;

use crate::ffi;
use crate::ids::{BodyId, SubShapeId, WorldKey};

/// Closest rigid-body intersection along a finite world-space segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// Body intersected by the segment.
    pub body: BodyId,
    /// Child shape intersected on the body.
    pub sub_shape: SubShapeId,
    /// Normalized distance from the segment origin in the inclusive range zero to one.
    pub fraction: f32,
    /// World-space intersection point.
    pub position: Vec3A,
    /// World-space surface normal at the intersection.
    pub normal: Vec3A,
}

pub(crate) fn hit_from_raw(value: ffi::RawRayHit, world: WorldKey) -> RayHit {
    let hit = value.0;
    RayHit {
        body: BodyId {
            raw: hit.body_id,
            world,
        },
        sub_shape: SubShapeId(hit.sub_shape_id),
        fraction: hit.fraction,
        position: ffi::safe_vec(hit.position),
        normal: ffi::safe_vec(hit.normal),
    }
}
