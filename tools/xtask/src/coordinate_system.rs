use glam::{Mat4, Vec3, Vec4};

/// Recipe identity for the glTF-to-Blackflower coordinate conversion.
pub(crate) const COOKER_RECIPE: &str =
    "gltf-rh-y-up-plus-z-forward-to-blackflower-rh-y-up-minus-z-forward-v1";

const AXIS_SIGNS: Vec3 = Vec3::new(-1.0, 1.0, -1.0);

/// Converts a glTF point or direction to Blackflower coordinates.
///
/// Both systems are right-handed and Y-up. Blackflower rotates the glTF basis
/// 180 degrees around Y so that +X points right and -Z points forward.
pub(crate) fn vector_from_gltf(value: Vec3) -> Vec3 {
    canonical_vector3(AXIS_SIGNS * value)
}

/// Converts a glTF tangent while preserving its handedness sign.
pub(crate) fn tangent_from_gltf(value: Vec4) -> Vec4 {
    let tangent = vector_from_gltf(value.truncate());
    Vec4::new(tangent.x, tangent.y, tangent.z, canonical_zero(value.w))
}

/// Changes the basis of a column-major glTF local matrix to Blackflower.
pub(crate) fn matrix_from_gltf(matrix: Mat4) -> Mat4 {
    let basis = Mat4::from_diagonal(AXIS_SIGNS.extend(1.0));
    let converted = basis * matrix * basis;
    Mat4::from_cols_array(&converted.to_cols_array().map(canonical_zero))
}

fn canonical_vector3(value: Vec3) -> Vec3 {
    Vec3::from_array(value.to_array().map(canonical_zero))
}

fn canonical_zero(value: f32) -> f32 {
    if value == 0.0 { 0.0 } else { value }
}

#[cfg(test)]
#[path = "../tests/unit/coordinate_system.rs"]
mod tests;
