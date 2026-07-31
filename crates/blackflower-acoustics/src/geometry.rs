use serde::{Deserialize, Serialize};

use crate::{Error, PositionMm, QuantizedTransform};

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

    pub(crate) fn transformed(self, transform: QuantizedTransform) -> Self {
        Self {
            vertices: self.vertices.map(|vertex| transform.apply(vertex)),
            material_index: self.material_index,
        }
    }
}

fn difference(left: PositionMm, right: PositionMm) -> [i64; 3] {
    [
        i64::from(left.x) - i64::from(right.x),
        i64::from(left.y) - i64::from(right.y),
        i64::from(left.z) - i64::from(right.z),
    ]
}

fn cross(left: [i64; 3], right: [i64; 3]) -> [i128; 3] {
    [
        i128::from(left[1]) * i128::from(right[2]) - i128::from(left[2]) * i128::from(right[1]),
        i128::from(left[2]) * i128::from(right[0]) - i128::from(left[0]) * i128::from(right[2]),
        i128::from(left[0]) * i128::from(right[1]) - i128::from(left[1]) * i128::from(right[0]),
    ]
}
