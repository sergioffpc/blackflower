use super::{matrix_from_gltf, tangent_from_gltf, vector_from_gltf};

#[test]
fn converts_gltf_vectors_to_blackflower_axes() {
    assert_float_bits_eq(vector_from_gltf([1.0, 2.0, 3.0]), [-1.0, 2.0, -3.0]);
    assert_float_bits_eq(
        tangent_from_gltf([1.0, 2.0, 3.0, -1.0]),
        [-1.0, 2.0, -3.0, -1.0],
    );
}

#[test]
fn changes_the_basis_of_local_matrices() {
    let gltf = [
        [1.0, 2.0, 3.0, 0.0],
        [4.0, 5.0, 6.0, 0.0],
        [7.0, 8.0, 9.0, 0.0],
        [10.0, 11.0, 12.0, 1.0],
    ];

    assert_float_bits_eq(
        matrix_from_gltf(gltf),
        [
            1.0, -2.0, 3.0, 0.0, -4.0, 5.0, -6.0, 0.0, 7.0, -8.0, 9.0, 0.0, -10.0, 11.0, -12.0, 1.0,
        ],
    );
}

#[test]
fn canonicalizes_signed_zero() {
    let converted = vector_from_gltf([-0.0, -0.0, 0.0]);
    assert_eq!(converted.map(f32::to_bits), [0, 0, 0]);
}

fn assert_float_bits_eq<const N: usize>(actual: [f32; N], expected: [f32; N]) {
    assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
}
