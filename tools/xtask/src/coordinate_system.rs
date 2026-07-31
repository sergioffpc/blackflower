/// Recipe identity for the glTF-to-Blackflower coordinate conversion.
pub(crate) const COOKER_RECIPE: &str =
    "gltf-rh-y-up-plus-z-forward-to-blackflower-rh-y-up-minus-z-forward-v1";

const AXIS_SIGNS: [f32; 4] = [-1.0, 1.0, -1.0, 1.0];

/// Converts a glTF point or direction to Blackflower coordinates.
///
/// Both systems are right-handed and Y-up. Blackflower rotates the glTF basis
/// 180 degrees around Y so that +X points right and -Z points forward.
pub(crate) fn vector_from_gltf([x, y, z]: [f32; 3]) -> [f32; 3] {
    [canonical_zero(-x), canonical_zero(y), canonical_zero(-z)]
}

/// Converts a glTF tangent while preserving its handedness sign.
pub(crate) fn tangent_from_gltf([x, y, z, w]: [f32; 4]) -> [f32; 4] {
    let [x, y, z] = vector_from_gltf([x, y, z]);
    [x, y, z, canonical_zero(w)]
}

/// Changes the basis of a column-major glTF local matrix to Blackflower.
pub(crate) fn matrix_from_gltf(matrix: [[f32; 4]; 4]) -> [f32; 16] {
    let mut converted = [0.0; 16];
    for (column, values) in matrix.into_iter().enumerate() {
        for (row, value) in values.into_iter().enumerate() {
            converted[column * 4 + row] =
                canonical_zero(value * AXIS_SIGNS[row] * AXIS_SIGNS[column]);
        }
    }
    converted
}

fn canonical_zero(value: f32) -> f32 {
    if value == 0.0 { 0.0 } else { value }
}

#[cfg(test)]
#[path = "../tests/unit/coordinate_system.rs"]
mod tests;
