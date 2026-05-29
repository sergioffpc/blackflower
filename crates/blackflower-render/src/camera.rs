use blackflower_core::math::{Mat4, Vec3};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,

    aspect: f32,
    yfov: f32,
    znear: f32,
    zfar: f32,
}

impl Camera {
    #[must_use]
    pub const fn new(aspect: f32, yfov: f32, znear: f32, zfar: f32) -> Self {
        Self {
            eye: Vec3::Z,
            target: Vec3::ZERO,
            up: Vec3::Y,
            aspect,
            yfov,
            znear,
            zfar,
        }
    }

    pub fn view_proj(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        let proj = Mat4::perspective_rh(self.yfov, self.aspect, self.znear, self.zfar);
        proj * view
    }
}

/// GPU-side representation of the camera.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

impl From<&Camera> for CameraUniform {
    fn from(value: &Camera) -> Self {
        Self {
            view_proj: value.view_proj().to_cols_array_2d(),
        }
    }
}

pub struct CameraResources {
    pub camera: Camera,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,

    uniform: CameraUniform,
    buffer: wgpu::Buffer,
}

impl CameraResources {
    pub fn new(device: &wgpu::Device, aspect: f32, yfov: f32, znear: f32, zfar: f32) -> Self {
        let camera = Camera::new(aspect, yfov, znear, zfar);
        let uniform = CameraUniform::from(&camera);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera uniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            camera,
            bind_group_layout,
            bind_group,
            uniform,
            buffer,
        }
    }

    pub fn update_buffer(&mut self, queue: &wgpu::Queue) {
        self.uniform = CameraUniform::from(&self.camera);
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[self.uniform]));
    }
}
