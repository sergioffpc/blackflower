use glam::Vec3A;

use crate::error::Error;
use crate::ffi;
use crate::ids::{BodyId, SubShapeId, WorldKey};

/// Lifecycle classification for a rigid-body contact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContactEventKind {
    /// The body/sub-shape pair started touching during the latest step.
    Added,
    /// The body/sub-shape pair remained touching during the latest step.
    Persisted,
    /// The body/sub-shape pair stopped touching during the latest step.
    Removed,
}

/// One pair of world-space points in a contact manifold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactPoint {
    /// Point on the first body's surface.
    pub position_on1: Vec3A,
    /// Point on the second body's surface.
    pub position_on2: Vec3A,
}

/// Geometry and material response data for an active contact.
#[derive(Debug, Clone, PartialEq)]
pub struct ContactManifold {
    /// Direction along which the second body is moved out of collision.
    pub normal: Vec3A,
    /// Required separation distance; negative values denote speculative contacts.
    pub penetration_depth: f32,
    /// Friction combined by the physics world for this body pair.
    pub combined_friction: f32,
    /// Restitution combined by the physics world for this body pair.
    pub combined_restitution: f32,
    /// Whether the contact is a sensor contact without collision response.
    pub is_sensor: bool,
    /// Canonically ordered pairs of surface points.
    pub points: Vec<ContactPoint>,
}

/// Immutable contact fact captured during the latest physics step.
#[derive(Debug, Clone, PartialEq)]
pub struct ContactEvent {
    /// Contact lifecycle classification.
    pub kind: ContactEventKind,
    /// First body in the physics world's canonical pair ordering.
    pub body1: BodyId,
    /// Second body in the physics world's canonical pair ordering.
    pub body2: BodyId,
    /// Child shape on the first body.
    pub sub_shape1: SubShapeId,
    /// Child shape on the second body.
    pub sub_shape2: SubShapeId,
    /// Manifold captured for added and persisted contacts.
    pub manifold: Option<ContactManifold>,
}

pub(crate) fn event_from_raw(
    value: ffi::RawContactEvent,
    world: WorldKey,
) -> Result<ContactEvent, Error> {
    let kind = event_kind(value.event.kind)?;
    let manifold = match kind {
        ContactEventKind::Added | ContactEventKind::Persisted => {
            Some(manifold_from_raw(value.event, value.points))
        }
        ContactEventKind::Removed => {
            if value.points.is_empty() {
                None
            } else {
                return Err(Error::NativeContract);
            }
        }
    };
    Ok(ContactEvent {
        kind,
        body1: BodyId {
            raw: value.event.body1_id,
            world,
        },
        body2: BodyId {
            raw: value.event.body2_id,
            world,
        },
        sub_shape1: SubShapeId(value.event.sub_shape1_id),
        sub_shape2: SubShapeId(value.event.sub_shape2_id),
        manifold,
    })
}

fn event_kind(value: u32) -> Result<ContactEventKind, Error> {
    match value {
        ffi::raw::BF_PHYSICS_CONTACT_ADDED => Ok(ContactEventKind::Added),
        ffi::raw::BF_PHYSICS_CONTACT_PERSISTED => Ok(ContactEventKind::Persisted),
        ffi::raw::BF_PHYSICS_CONTACT_REMOVED => Ok(ContactEventKind::Removed),
        _ => Err(Error::NativeContract),
    }
}

fn manifold_from_raw(
    event: ffi::raw::BFPhysicsContactEvent,
    points: Vec<ffi::raw::BFPhysicsContactPoint>,
) -> ContactManifold {
    ContactManifold {
        normal: ffi::safe_vec(event.normal),
        penetration_depth: event.penetration_depth,
        combined_friction: event.combined_friction,
        combined_restitution: event.combined_restitution,
        is_sensor: event.is_sensor != 0,
        points: points
            .into_iter()
            .map(|point| ContactPoint {
                position_on1: ffi::safe_vec(point.position_on1),
                position_on2: ffi::safe_vec(point.position_on2),
            })
            .collect(),
    }
}
