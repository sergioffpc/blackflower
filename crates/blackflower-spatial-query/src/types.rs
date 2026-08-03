use glam::Vec3A;

use crate::Error;

/// Identifier assigned by one committed spatial scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GeometryId(pub u32);

/// Triangle index within one geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrimitiveId(pub u32);

/// Optional instance identifier reported by an instanced hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstanceId(pub u32);

/// One validated counter-clockwise triangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle {
    vertices: [Vec3A; 3],
}

impl Triangle {
    /// Validate three finite, non-degenerate vertices.
    pub fn new(vertices: [Vec3A; 3]) -> Result<Self, Error> {
        if !vertices.iter().all(|vertex| vertex.is_finite()) {
            return Err(Error::InvalidTriangle);
        }
        let left = vertices[1] - vertices[0];
        let right = vertices[2] - vertices[0];
        if left.cross(right).length_squared() <= 0.0 {
            return Err(Error::InvalidTriangle);
        }
        Ok(Self { vertices })
    }

    /// Counter-clockwise vertices.
    #[must_use]
    pub const fn vertices(self) -> [Vec3A; 3] {
        self.vertices
    }
}

/// One surface crossing returned by a scene query.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceHit {
    distance: f32,
    fraction: f32,
    geometric_normal: [f32; 3],
    barycentric_u: f32,
    barycentric_v: f32,
    geometry_id: u32,
    primitive_id: u32,
    instance_id: u32,
}

impl SurfaceHit {
    /// Distance from the segment start in the caller's coordinate unit.
    #[must_use]
    pub const fn distance(self) -> f32 {
        self.distance
    }

    /// Parametric position from zero at the start to one at the end.
    #[must_use]
    pub const fn fraction(self) -> f32 {
        self.fraction
    }

    /// Unnormalized geometric normal in object space.
    #[must_use]
    pub fn geometric_normal(self) -> Vec3A {
        Vec3A::from_array(self.geometric_normal)
    }

    /// Triangle barycentric coordinates `(u, v)`.
    #[must_use]
    pub const fn barycentric(self) -> [f32; 2] {
        [self.barycentric_u, self.barycentric_v]
    }

    /// Geometry containing the intersected primitive.
    #[must_use]
    pub const fn geometry_id(self) -> GeometryId {
        GeometryId(self.geometry_id)
    }

    /// Triangle index within the geometry.
    #[must_use]
    pub const fn primitive_id(self) -> PrimitiveId {
        PrimitiveId(self.primitive_id)
    }

    /// First instance in the hit stack, if the primitive was instanced.
    #[must_use]
    pub const fn instance_id(self) -> Option<InstanceId> {
        if self.instance_id == u32::MAX {
            None
        } else {
            Some(InstanceId(self.instance_id))
        }
    }
}
