use blackflower_math::{Mat4, Vec3, components::Transform};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];

    pub const fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ModelUniform {
    model: [[f32; 4]; 4],
}

impl From<&Transform> for ModelUniform {
    fn from(value: &Transform) -> Self {
        let m = Mat4::from_scale_rotation_translation(Vec3::ONE, value.rotation, value.translation);
        Self {
            model: m.to_cols_array_2d(),
        }
    }
}

pub struct GeometryResources {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl GeometryResources {
    pub fn new(device: &wgpu::Device, vertices: &[Vertex], indices: &[u32]) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let index_count = indices.len() as u32;

        Self {
            vertex_buffer,
            index_buffer,
            index_count,
        }
    }
}

#[rustfmt::skip]
pub const CUBE_VERTICES: &[Vertex] = &[
    Vertex { position: [ 0.5, -0.5, -0.5], normal: [1.0, 0.0, 0.0], color: [1.0, 0.0, 0.0] },
    Vertex { position: [ 0.5,  0.5, -0.5], normal: [1.0, 0.0, 0.0], color: [1.0, 0.0, 0.0] },
    Vertex { position: [ 0.5,  0.5,  0.5], normal: [1.0, 0.0, 0.0], color: [1.0, 0.0, 0.0] },
    Vertex { position: [ 0.5, -0.5,  0.5], normal: [1.0, 0.0, 0.0], color: [1.0, 0.0, 0.0] },

    Vertex { position: [-0.5, -0.5,  0.5], normal: [-1.0, 0.0, 0.0], color: [0.0, 1.0, 1.0] },
    Vertex { position: [-0.5,  0.5,  0.5], normal: [-1.0, 0.0, 0.0], color: [0.0, 1.0, 1.0] },
    Vertex { position: [-0.5,  0.5, -0.5], normal: [-1.0, 0.0, 0.0], color: [0.0, 1.0, 1.0] },
    Vertex { position: [-0.5, -0.5, -0.5], normal: [-1.0, 0.0, 0.0], color: [0.0, 1.0, 1.0] },

    Vertex { position: [-0.5,  0.5, -0.5], normal: [0.0, 1.0, 0.0], color: [0.0, 1.0, 0.0] },
    Vertex { position: [-0.5,  0.5,  0.5], normal: [0.0, 1.0, 0.0], color: [0.0, 1.0, 0.0] },
    Vertex { position: [ 0.5,  0.5,  0.5], normal: [0.0, 1.0, 0.0], color: [0.0, 1.0, 0.0] },
    Vertex { position: [ 0.5,  0.5, -0.5], normal: [0.0, 1.0, 0.0], color: [0.0, 1.0, 0.0] },

    Vertex { position: [-0.5, -0.5,  0.5], normal: [0.0, -1.0, 0.0], color: [1.0, 0.0, 1.0] },
    Vertex { position: [-0.5, -0.5, -0.5], normal: [0.0, -1.0, 0.0], color: [1.0, 0.0, 1.0] },
    Vertex { position: [ 0.5, -0.5, -0.5], normal: [0.0, -1.0, 0.0], color: [1.0, 0.0, 1.0] },
    Vertex { position: [ 0.5, -0.5,  0.5], normal: [0.0, -1.0, 0.0], color: [1.0, 0.0, 1.0] },

    Vertex { position: [-0.5, -0.5,  0.5], normal: [0.0, 0.0, 1.0], color: [0.0, 0.0, 1.0] },
    Vertex { position: [ 0.5, -0.5,  0.5], normal: [0.0, 0.0, 1.0], color: [0.0, 0.0, 1.0] },
    Vertex { position: [ 0.5,  0.5,  0.5], normal: [0.0, 0.0, 1.0], color: [0.0, 0.0, 1.0] },
    Vertex { position: [-0.5,  0.5,  0.5], normal: [0.0, 0.0, 1.0], color: [0.0, 0.0, 1.0] },

    Vertex { position: [ 0.5, -0.5, -0.5], normal: [0.0, 0.0, -1.0], color: [1.0, 1.0, 0.0] },
    Vertex { position: [-0.5, -0.5, -0.5], normal: [0.0, 0.0, -1.0], color: [1.0, 1.0, 0.0] },
    Vertex { position: [-0.5,  0.5, -0.5], normal: [0.0, 0.0, -1.0], color: [1.0, 1.0, 0.0] },
    Vertex { position: [ 0.5,  0.5, -0.5], normal: [0.0, 0.0, -1.0], color: [1.0, 1.0, 0.0] },
];

#[rustfmt::skip]
pub const CUBE_INDICES: &[u32] = &[
     0,  1,  2,    0,  2,  3,  // +X
     4,  5,  6,    4,  6,  7,  // -X
     8,  9, 10,    8, 10, 11,  // +Y
    12, 13, 14,   12, 14, 15,  // -Y
    16, 17, 18,   16, 18, 19,  // +Z
    20, 21, 22,   20, 22, 23,  // -Z
];
