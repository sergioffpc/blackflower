use glam::Quat;
use gltf::scene::Transform;

use super::{normalize_quaternion, transform};

#[test]
fn normalizes_quaternion_before_composing_the_canonical_matrix() -> anyhow::Result<()> {
    let cooked = transform(Transform::Decomposed {
        translation: [1.0, 2.0, 3.0],
        rotation: [0.0, 2.0, 0.0, 2.0],
        scale: [1.0, 1.0, 1.0],
    })?;

    let matrix = cooked.to_cols_array();
    assert_approximately_equal(matrix[0], 0.0);
    assert_approximately_equal(matrix[2], -1.0);
    assert_approximately_equal(matrix[8], 1.0);
    assert_approximately_equal(matrix[10], 0.0);
    assert_eq!(&matrix[12..16], &[-1.0, 2.0, -3.0, 1.0]);
    Ok(())
}

#[test]
fn quaternion_sign_does_not_change_cooked_matrix() -> anyhow::Result<()> {
    let positive = transform(Transform::Decomposed {
        translation: [0.0; 3],
        rotation: [0.0, 2.0, 0.0, 2.0],
        scale: [1.0; 3],
    })?;
    let negative = transform(Transform::Decomposed {
        translation: [0.0; 3],
        rotation: [0.0, -2.0, 0.0, -2.0],
        scale: [1.0; 3],
    })?;

    assert_eq!(positive, negative);
    Ok(())
}

#[test]
fn rejects_zero_and_non_finite_quaternions() {
    assert!(normalize_quaternion(Quat::from_xyzw(0.0, 0.0, 0.0, 0.0)).is_err());
    assert!(normalize_quaternion(Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0)).is_err());
}

#[test]
fn converts_authored_matrices_without_decomposition() -> anyhow::Result<()> {
    let cooked = transform(Transform::Matrix {
        matrix: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.5, 0.0, 1.0, 0.0],
            [1.0, 2.0, 3.0, 1.0],
        ],
    })?;

    assert_float_bits_eq(
        cooked.to_cols_array(),
        [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5, 0.0, 1.0, 0.0, -1.0, 2.0, -3.0, 1.0,
        ],
    );
    Ok(())
}

fn assert_approximately_equal(left: f32, right: f32) {
    assert!((left - right).abs() <= 1.0e-6, "{left} != {right}");
}

fn assert_float_bits_eq<const N: usize>(actual: [f32; N], expected: [f32; N]) {
    assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
}
