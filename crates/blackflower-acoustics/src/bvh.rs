use serde::{Deserialize, Serialize};

use crate::{AabbMm, Error, PositionMm, QuantizedTransform};

const LEAF_TRIANGLES: usize = 4;

/// One millimetre-quantized acoustic triangle and its material table index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantizedTriangle {
    /// Counter-clockwise vertices.
    pub vertices: [PositionMm; 3],
    /// Index into an [`crate::AcousticMaterialLibrary`].
    pub material_index: u16,
}

impl QuantizedTriangle {
    /// Construct a non-degenerate triangle.
    pub fn new(vertices: [PositionMm; 3], material_index: u16) -> Result<Self, Error> {
        let left = difference(vertices[1], vertices[0]);
        let right = difference(vertices[2], vertices[0]);
        if cross(left, right) == [0; 3] {
            return Err(Error::InvalidField("acoustic triangle"));
        }
        Ok(Self {
            vertices,
            material_index,
        })
    }

    /// Inclusive triangle bounds.
    #[must_use]
    pub fn bounds(self) -> AabbMm {
        let [a, b, c] = self.vertices;
        AabbMm {
            min: PositionMm::new(
                a.x.min(b.x).min(c.x),
                a.y.min(b.y).min(c.y),
                a.z.min(b.z).min(c.z),
            ),
            max: PositionMm::new(
                a.x.max(b.x).max(c.x),
                a.y.max(b.y).max(c.y),
                a.z.max(b.z).max(c.z),
            ),
        }
    }

    pub(crate) fn transformed(self, transform: QuantizedTransform) -> Self {
        Self {
            vertices: self.vertices.map(|vertex| transform.apply(vertex)),
            material_index: self.material_index,
        }
    }
}

/// One deterministic binary BVH node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BvhNode {
    /// Node bounds.
    pub bounds: AabbMm,
    /// First child for branches or first triangle for leaves.
    pub first: u32,
    /// Second child for branches or number of triangles for leaves.
    pub second: u32,
    /// `true` when `first..first+second` addresses the triangle-order array.
    pub leaf: bool,
}

/// Deterministically built acceleration structure over acoustic-only geometry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticBvh {
    /// Depth-first node table.
    pub nodes: Vec<BvhNode>,
    /// Stable triangle indices addressed by leaves.
    pub triangle_order: Vec<u32>,
}

impl AcousticBvh {
    /// Build using stable longest-axis centroid splits and triangle-index ties.
    pub fn build(triangles: &[QuantizedTriangle]) -> Result<Self, Error> {
        if triangles.is_empty() {
            return Ok(Self {
                nodes: Vec::new(),
                triangle_order: Vec::new(),
            });
        }
        if u32::try_from(triangles.len()).is_err() {
            return Err(Error::ResourceLimit("BVH triangles"));
        }
        let mut output = Self {
            nodes: Vec::with_capacity(triangles.len().saturating_mul(2)),
            triangle_order: Vec::with_capacity(triangles.len()),
        };
        let indices = (0..triangles.len()).collect::<Vec<_>>();
        let _root = build_node(triangles, indices, &mut output)?;
        Ok(output)
    }

    /// Return canonically ordered surfaces intersected by a segment.
    #[allow(
        clippy::too_many_lines,
        reason = "the bounded traversal keeps stack, hit selection, and stable ordering together"
    )]
    pub fn intersect_segment(
        &self,
        triangles: &[QuantizedTriangle],
        start: PositionMm,
        end: PositionMm,
        max_hits: usize,
        output: &mut Vec<SurfaceHit>,
    ) {
        output.clear();
        if self.nodes.is_empty() || max_hits == 0 {
            return;
        }
        let mut stack = [0_u32; 128];
        let mut stack_len = 1_usize;
        while stack_len != 0 {
            stack_len -= 1;
            let node_index = stack[stack_len];
            let Some(node) = usize::try_from(node_index)
                .ok()
                .and_then(|index| self.nodes.get(index))
            else {
                continue;
            };
            if !segment_intersects_aabb(start, end, node.bounds) {
                continue;
            }
            if node.leaf {
                let Some(begin) = usize::try_from(node.first).ok() else {
                    continue;
                };
                let Some(count) = usize::try_from(node.second).ok() else {
                    continue;
                };
                let Some(indices) = self.triangle_order.get(begin..begin.saturating_add(count))
                else {
                    continue;
                };
                for triangle_index in indices {
                    let Some(triangle) = usize::try_from(*triangle_index)
                        .ok()
                        .and_then(|index| triangles.get(index))
                    else {
                        continue;
                    };
                    if let Some(distance_mm) = segment_triangle_distance(start, end, *triangle) {
                        let hit = SurfaceHit {
                            distance_mm,
                            triangle_index: *triangle_index,
                            material_index: triangle.material_index,
                        };
                        if output.len() < max_hits {
                            output.push(hit);
                        } else if let Some((worst_index, worst)) = output
                            .iter()
                            .enumerate()
                            .max_by_key(|(_index, value)| (value.distance_mm, value.triangle_index))
                            && (hit.distance_mm, hit.triangle_index)
                                < (worst.distance_mm, worst.triangle_index)
                        {
                            output[worst_index] = hit;
                        }
                    }
                }
            } else {
                // Push the larger child first so the smaller stable node ID is visited first.
                if stack_len.saturating_add(2) <= stack.len() {
                    stack[stack_len] = node.second;
                    stack[stack_len + 1] = node.first;
                    stack_len += 2;
                }
            }
        }
        output.sort_by_key(|hit| (hit.distance_mm, hit.triangle_index));
        output.dedup_by_key(|hit| hit.triangle_index);
    }
}

/// One stable surface crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceHit {
    /// Distance from the segment origin.
    pub distance_mm: u64,
    /// Stable triangle index.
    pub triangle_index: u32,
    /// Material table index.
    pub material_index: u16,
}

#[allow(
    clippy::too_many_lines,
    reason = "deterministic BVH partitioning is easier to audit as one recursive operation"
)]
fn build_node(
    triangles: &[QuantizedTriangle],
    mut indices: Vec<usize>,
    output: &mut AcousticBvh,
) -> Result<u32, Error> {
    let node_index =
        u32::try_from(output.nodes.len()).map_err(|_error| Error::ResourceLimit("BVH nodes"))?;
    let bounds = indices
        .iter()
        .copied()
        .map(|index| triangles[index].bounds())
        .reduce(AabbMm::union)
        .ok_or(Error::InvalidField("BVH node"))?;
    output.nodes.push(BvhNode {
        bounds,
        first: 0,
        second: 0,
        leaf: true,
    });
    if indices.len() <= LEAF_TRIANGLES {
        indices.sort_unstable();
        let first = u32::try_from(output.triangle_order.len())
            .map_err(|_error| Error::ResourceLimit("BVH order"))?;
        for index in indices {
            output.triangle_order.push(
                u32::try_from(index).map_err(|_error| Error::ResourceLimit("BVH triangles"))?,
            );
        }
        output.nodes[usize::try_from(node_index).unwrap_or(0)] = BvhNode {
            bounds,
            first,
            second: u32::try_from(output.triangle_order.len())
                .map_err(|_error| Error::ResourceLimit("BVH order"))?
                .saturating_sub(first),
            leaf: true,
        };
        return Ok(node_index);
    }
    let extents = [
        i64::from(bounds.max.x) - i64::from(bounds.min.x),
        i64::from(bounds.max.y) - i64::from(bounds.min.y),
        i64::from(bounds.max.z) - i64::from(bounds.min.z),
    ];
    let axis = (0..3)
        .max_by_key(|axis| (extents[*axis], 2_usize.saturating_sub(*axis)))
        .unwrap_or(0);
    indices.sort_by_key(|index| (centroid_axis(triangles[*index], axis), *index));
    let right = indices.split_off(indices.len() / 2);
    let left_index = build_node(triangles, indices, output)?;
    let right_index = build_node(triangles, right, output)?;
    output.nodes[usize::try_from(node_index).unwrap_or(0)] = BvhNode {
        bounds,
        first: left_index,
        second: right_index,
        leaf: false,
    };
    Ok(node_index)
}

fn centroid_axis(triangle: QuantizedTriangle, axis: usize) -> i64 {
    triangle.vertices.iter().fold(0_i64, |total, vertex| {
        total
            + i64::from(match axis {
                0 => vertex.x,
                1 => vertex.y,
                _ => vertex.z,
            })
    })
}

fn segment_intersects_aabb(start: PositionMm, end: PositionMm, bounds: AabbMm) -> bool {
    let origin = [f64::from(start.x), f64::from(start.y), f64::from(start.z)];
    let direction = [
        f64::from(end.x) - origin[0],
        f64::from(end.y) - origin[1],
        f64::from(end.z) - origin[2],
    ];
    let minimum = [
        f64::from(bounds.min.x),
        f64::from(bounds.min.y),
        f64::from(bounds.min.z),
    ];
    let maximum = [
        f64::from(bounds.max.x),
        f64::from(bounds.max.y),
        f64::from(bounds.max.z),
    ];
    let mut near = 0.0_f64;
    let mut far = 1.0_f64;
    for axis in 0..3 {
        if direction[axis].abs() < f64::EPSILON {
            if origin[axis] < minimum[axis] || origin[axis] > maximum[axis] {
                return false;
            }
            continue;
        }
        let inverse = 1.0 / direction[axis];
        let mut first = (minimum[axis] - origin[axis]) * inverse;
        let mut second = (maximum[axis] - origin[axis]) * inverse;
        if first > second {
            core::mem::swap(&mut first, &mut second);
        }
        near = near.max(first);
        far = far.min(second);
        if near > far {
            return false;
        }
    }
    true
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "intersection distance is clamped to the finite segment before integer quantization"
)]
#[allow(
    clippy::cast_precision_loss,
    reason = "authored millimetre path lengths are bounded far below the exact f64 integer range"
)]
fn segment_triangle_distance(
    start: PositionMm,
    end: PositionMm,
    triangle: QuantizedTriangle,
) -> Option<u64> {
    let origin = [f64::from(start.x), f64::from(start.y), f64::from(start.z)];
    let direction = [
        f64::from(end.x) - origin[0],
        f64::from(end.y) - origin[1],
        f64::from(end.z) - origin[2],
    ];
    let v0 = to_f64(triangle.vertices[0]);
    let edge1 = subtract(to_f64(triangle.vertices[1]), v0);
    let edge2 = subtract(to_f64(triangle.vertices[2]), v0);
    let h = cross_f64(direction, edge2);
    let determinant = dot(edge1, h);
    if determinant.abs() < 1.0e-9 {
        return None;
    }
    let inverse = 1.0 / determinant;
    let s = subtract(origin, v0);
    let u = inverse * dot(s, h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross_f64(s, edge1);
    let v = inverse * dot(direction, q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let fraction = inverse * dot(edge2, q);
    if !(0.0..=1.0).contains(&fraction) {
        return None;
    }
    let length = start.distance(end);
    Some((fraction * length as f64).round().clamp(0.0, length as f64) as u64)
}

fn difference(left: PositionMm, right: PositionMm) -> [i64; 3] {
    [
        i64::from(left.x) - i64::from(right.x),
        i64::from(left.y) - i64::from(right.y),
        i64::from(left.z) - i64::from(right.z),
    ]
}

fn cross(left: [i64; 3], right: [i64; 3]) -> [i64; 3] {
    [
        left[1]
            .saturating_mul(right[2])
            .saturating_sub(left[2].saturating_mul(right[1])),
        left[2]
            .saturating_mul(right[0])
            .saturating_sub(left[0].saturating_mul(right[2])),
        left[0]
            .saturating_mul(right[1])
            .saturating_sub(left[1].saturating_mul(right[0])),
    ]
}

fn to_f64(value: PositionMm) -> [f64; 3] {
    [f64::from(value.x), f64::from(value.y), f64::from(value.z)]
}

fn subtract(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn cross_f64(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bvh_build_and_hits_are_stable() -> Result<(), Error> {
        let triangles = vec![QuantizedTriangle::new(
            [
                PositionMm::new(1_000, -1_000, -1_000),
                PositionMm::new(1_000, 1_000, -1_000),
                PositionMm::new(1_000, 0, 1_000),
            ],
            2,
        )?];
        let first = AcousticBvh::build(&triangles)?;
        let second = AcousticBvh::build(&triangles)?;
        assert_eq!(first, second);
        let mut hits = Vec::new();
        first.intersect_segment(
            &triangles,
            PositionMm::new(0, 0, 0),
            PositionMm::new(2_000, 0, 0),
            8,
            &mut hits,
        );
        assert_eq!(hits[0].distance_mm, 1_000);
        assert_eq!(hits[0].material_index, 2);
        Ok(())
    }
}
