use blackflower_core::math::{Mat4, Vec3};
use bytemuck::{Pod, Zeroable};

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
    view_proj: [[f32; 4]; 4],
}

impl From<&Camera> for CameraUniform {
    fn from(value: &Camera) -> Self {
        Self {
            view_proj: value.view_proj().to_cols_array_2d(),
        }
    }
}
