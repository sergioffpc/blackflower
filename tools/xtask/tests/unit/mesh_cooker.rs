use super::assemble_vertices;

#[test]
fn converts_mesh_vertex_channels_to_blackflower_coordinates() {
    let vertices = assemble_vertices(
        &[[1.0, 2.0, 3.0]],
        Some(&[[0.25, 0.5, 0.75]]),
        Some(&[[1.0, 0.0, -1.0, -1.0]]),
        Some(&[[0.25, 0.75]]),
    );

    assert_float_bits_eq(vertices[0].position, [-1.0, 2.0, -3.0]);
    assert_float_bits_eq(vertices[0].normal, [-0.25, 0.5, -0.75]);
    assert_float_bits_eq(vertices[0].tangent, [-1.0, 0.0, 1.0, -1.0]);
    assert_float_bits_eq(vertices[0].texcoord_0, [0.25, 0.75]);
}

fn assert_float_bits_eq<const N: usize>(actual: [f32; N], expected: [f32; N]) {
    assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
}
