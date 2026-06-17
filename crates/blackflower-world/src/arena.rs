use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

/// Axis-aligned bounding box stored as raw float arrays (no glam dependency).
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    /// Construct from center point and per-axis half-extents.
    #[must_use]
    pub fn from_center_half(center: [f32; 3], half: [f32; 3]) -> Self {
        Self {
            min: [
                center[0] - half[0],
                center[1] - half[1],
                center[2] - half[2],
            ],
            max: [
                center[0] + half[0],
                center[1] + half[1],
                center[2] + half[2],
            ],
        }
    }

    /// True when the two boxes penetrate (touching edges do not count).
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.min[0] < other.max[0]
            && self.max[0] > other.min[0]
            && self.min[1] < other.max[1]
            && self.max[1] > other.min[1]
            && self.min[2] < other.max[2]
            && self.max[2] > other.min[2]
    }

    /// True when the point `p` is strictly inside the box.
    #[must_use]
    pub fn contains(self, p: [f32; 3]) -> bool {
        p[0] > self.min[0]
            && p[0] < self.max[0]
            && p[1] > self.min[1]
            && p[1] < self.max[1]
            && p[2] > self.min[2]
            && p[2] < self.max[2]
    }
}

/// Static arena geometry loaded from a RON file.
#[derive(Clone, Debug, Deserialize)]
pub struct Arena {
    pub id: String,
    /// Solid walls/floor/ceiling the player cannot pass through.
    pub walls: Vec<Aabb>,
    /// World-space positions where players may spawn (Y = eye height above floor).
    pub spawn_points: Vec<[f32; 3]>,
}

impl Arena {
    pub fn load<P>(path: P) -> anyhow::Result<Self>
    where
        P: AsRef<Path>,
    {
        let src = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("reading arena file {}", path.as_ref().display()))?;
        ron::from_str(&src)
            .map_err(|e| anyhow::anyhow!("arena parse error in {}: {e}", path.as_ref().display()))
    }

    /// Move `position` by `displacement`, sliding along walls axis-by-axis.
    /// Returns the new position after collision resolution.
    #[must_use]
    pub fn collide_and_slide(
        &self,
        position: [f32; 3],
        half_extents: [f32; 3],
        displacement: [f32; 3],
    ) -> [f32; 3] {
        let mut pos = position;

        let try_x = [pos[0] + displacement[0], pos[1], pos[2]];
        if !self.any_overlap(Aabb::from_center_half(try_x, half_extents)) {
            pos[0] = try_x[0];
        }

        let try_y = [pos[0], pos[1] + displacement[1], pos[2]];
        if !self.any_overlap(Aabb::from_center_half(try_y, half_extents)) {
            pos[1] = try_y[1];
        }

        let try_z = [pos[0], pos[1], pos[2] + displacement[2]];
        if !self.any_overlap(Aabb::from_center_half(try_z, half_extents)) {
            pos[2] = try_z[2];
        }

        pos
    }

    fn any_overlap(&self, entity: Aabb) -> bool {
        self.walls.iter().any(|w| entity.overlaps(*w))
    }
}
