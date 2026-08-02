use glam::{Quat, Vec3A};

use crate::Error;
use crate::types::{validate_rotation, validate_vector};

/// Maximum number of source points accepted for one convex hull.
///
/// Jolt supports at most this many points in the resulting hull. Keeping the
/// source bounded to the same value makes cooking and runtime costs predictable.
pub const MAX_CONVEX_HULL_POINTS: usize =
    crate::ffi::raw::BF_PHYSICS_MAX_CONVEX_HULL_POINTS as usize;

/// Collision geometry owned by safe Rust until a body is created.
///
/// Complex geometry is copied into a reference-counted Jolt shape during body
/// creation. No Rust slice or C++ handle is retained by the public API.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape(pub(crate) ShapeKind);

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ShapeKind {
    Sphere {
        radius: f32,
    },
    Box {
        half_extent: Vec3A,
    },
    Capsule {
        half_height: f32,
        radius: f32,
    },
    ConvexHull {
        points: Vec<Vec3A>,
    },
    Compound {
        children: Vec<CompoundShapeChild>,
    },
    TriangleMesh {
        vertices: Vec<Vec3A>,
        triangles: Vec<[u32; 3]>,
    },
}

/// One transformed child in an immutable compound collision shape.
#[derive(Debug, Clone, PartialEq)]
pub struct CompoundShapeChild {
    pub(crate) shape: Shape,
    pub(crate) position: Vec3A,
    pub(crate) rotation: Quat,
}

impl CompoundShapeChild {
    /// Place a child shape at the compound origin with identity rotation.
    #[must_use]
    pub fn new(shape: Shape) -> Self {
        Self {
            shape,
            position: Vec3A::ZERO,
            rotation: Quat::IDENTITY,
        }
    }

    /// Set the child's finite position relative to the compound origin.
    pub fn with_position(mut self, position: Vec3A) -> Result<Self, Error> {
        self.position = validate_vector(position)?;
        Ok(self)
    }

    /// Set the child's finite, normalized rotation relative to the compound.
    pub fn with_rotation(mut self, rotation: Quat) -> Result<Self, Error> {
        self.rotation = validate_rotation(rotation)?;
        Ok(self)
    }
}

impl Shape {
    /// Construct a sphere with a finite, positive radius.
    pub fn sphere(radius: f32) -> Result<Self, Error> {
        positive(radius)
            .then_some(Self(ShapeKind::Sphere { radius }))
            .ok_or(Error::InvalidShape)
    }

    /// Construct a box with finite, positive half extents.
    pub fn cuboid(half_extent: Vec3A) -> Result<Self, Error> {
        validate_vector(half_extent).map_err(|_error| Error::InvalidShape)?;
        (!half_extent.cmple(Vec3A::ZERO).any())
            .then_some(Self(ShapeKind::Box { half_extent }))
            .ok_or(Error::InvalidShape)
    }

    /// Construct a Y-axis capsule from its cylinder half-height and radius.
    pub fn capsule(half_height: f32, radius: f32) -> Result<Self, Error> {
        (positive(half_height) && positive(radius))
            .then_some(Self(ShapeKind::Capsule {
                half_height,
                radius,
            }))
            .ok_or(Error::InvalidShape)
    }

    /// Construct a convex hull from four to [`MAX_CONVEX_HULL_POINTS`] points.
    ///
    /// Points may be interior to the hull. Jolt rejects coplanar, collinear, or
    /// otherwise degenerate point sets when the body is created.
    pub fn convex_hull(points: impl Into<Vec<Vec3A>>) -> Result<Self, Error> {
        let points = points.into();
        if !(4..=MAX_CONVEX_HULL_POINTS).contains(&points.len())
            || points.iter().any(|point| !point.is_finite())
        {
            return Err(Error::InvalidShape);
        }
        Ok(Self(ShapeKind::ConvexHull { points }))
    }

    /// Construct an immutable, flat compound from transformed child shapes.
    ///
    /// Nested compounds are rejected; flatten them in the cooker so sub-shape
    /// identity and construction cost remain predictable.
    pub fn compound(children: impl Into<Vec<CompoundShapeChild>>) -> Result<Self, Error> {
        let children = children.into();
        if children.is_empty()
            || u32::try_from(children.len()).is_err()
            || children.iter().any(|child| child.shape.is_compound())
        {
            return Err(Error::InvalidShape);
        }
        Ok(Self(ShapeKind::Compound { children }))
    }

    /// Construct an indexed, single-sided triangle mesh for static collision.
    ///
    /// Triangle vertices must be counter-clockwise when viewed from the
    /// colliding side. Degenerate geometry is rejected when the body is created.
    pub fn triangle_mesh(
        vertices: impl Into<Vec<Vec3A>>,
        triangles: impl Into<Vec<[u32; 3]>>,
    ) -> Result<Self, Error> {
        let vertices = vertices.into();
        let triangles = triangles.into();
        validate_mesh(&vertices, &triangles)?;
        Ok(Self(ShapeKind::TriangleMesh {
            vertices,
            triangles,
        }))
    }

    pub(crate) const fn kind(&self) -> &ShapeKind {
        &self.0
    }

    pub(crate) fn requires_static_body(&self) -> bool {
        match self.kind() {
            ShapeKind::TriangleMesh { .. } => true,
            ShapeKind::Compound { children } => children
                .iter()
                .any(|child| child.shape.requires_static_body()),
            ShapeKind::Sphere { .. }
            | ShapeKind::Box { .. }
            | ShapeKind::Capsule { .. }
            | ShapeKind::ConvexHull { .. } => false,
        }
    }

    const fn is_compound(&self) -> bool {
        matches!(&self.0, ShapeKind::Compound { .. })
    }
}

fn validate_mesh(vertices: &[Vec3A], triangles: &[[u32; 3]]) -> Result<(), Error> {
    let counts_fit =
        u32::try_from(vertices.len()).is_ok() && u32::try_from(triangles.len()).is_ok();
    if vertices.len() < 3
        || triangles.is_empty()
        || !counts_fit
        || vertices.iter().any(|vertex| !vertex.is_finite())
        || triangles
            .iter()
            .any(|triangle| invalid_triangle(*triangle, vertices.len()))
    {
        return Err(Error::InvalidShape);
    }
    Ok(())
}

fn invalid_triangle(triangle: [u32; 3], vertex_count: usize) -> bool {
    triangle[0] == triangle[1]
        || triangle[1] == triangle[2]
        || triangle[2] == triangle[0]
        || triangle
            .into_iter()
            .any(|index| usize::try_from(index).map_or(true, |index| index >= vertex_count))
}

fn positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}
