use std::num::NonZeroU32;

use blackflower_rendering::{
    RenderFrame, RenderFrameId, RenderInstance, RenderView, ResourceHandle,
};
use glam::camera::rh::{proj::directx, view};
use glam::{DQuat, DVec3, Mat4, Quat, Vec3};

use crate::MovementProxy;

const PRIMARY_VIEW_ID: u64 = 1;
const DEFAULT_LAYER_MASK: u64 = 1;
const CAMERA_VERTICAL_OFFSET_METERS: f32 = 2.0;
const CAMERA_DISTANCE_METERS: f32 = 5.0;
const CAMERA_LOOK_AHEAD_METERS: f32 = 2.0;
const VERTICAL_FIELD_OF_VIEW_RADIANS: f32 = 60.0_f32.to_radians();
const NEAR_PLANE_METERS: f32 = 0.1;
const FAR_PLANE_METERS: f32 = 1_000.0;

/// Presentation-owned binding from the local movement proxy to one model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalVisualBinding {
    resource: ResourceHandle,
    layer_mask: u64,
}

impl LocalVisualBinding {
    /// Bind a model resource to the default gameplay visibility layer.
    #[must_use]
    pub const fn new(resource: ResourceHandle) -> Self {
        Self {
            resource,
            layer_mask: DEFAULT_LAYER_MASK,
        }
    }

    /// Return the persistent renderer-independent resource identity.
    #[must_use]
    pub const fn resource(self) -> ResourceHandle {
        self.resource
    }

    /// Return the semantic visibility layer.
    #[must_use]
    pub const fn layer_mask(self) -> u64 {
        self.layer_mask
    }
}

/// Non-empty drawable area captured for one presentation frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationViewport {
    width: NonZeroU32,
    height: NonZeroU32,
}

impl PresentationViewport {
    /// Validate a drawable viewport.
    ///
    /// # Errors
    ///
    /// Returns an error when either dimension is zero.
    pub fn new(width: u32, height: u32) -> Result<Self, PresentationSceneError> {
        Ok(Self {
            width: NonZeroU32::new(width).ok_or(PresentationSceneError::EmptyViewport)?,
            height: NonZeroU32::new(height).ok_or(PresentationSceneError::EmptyViewport)?,
        })
    }

    /// Return the drawable width in physical pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width.get()
    }

    /// Return the drawable height in physical pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height.get()
    }
}

/// Failure while resolving presentation-owned visual state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PresentationSceneError {
    /// A drawable viewport must have non-zero dimensions.
    #[error("presentation viewport dimensions must be non-zero")]
    EmptyViewport,
    /// Presentation-owned scene storage was poisoned by a previous panic.
    #[error("presentation scene state is unavailable")]
    StateUnavailable,
    /// A finite prediction transform could not be represented by rendering floats.
    #[error("presentation transform cannot be represented as finite f32 values")]
    TransformOutOfRange,
}

#[derive(Debug, Clone)]
struct SceneOutput {
    view: RenderView,
    instance: RenderInstance,
}

#[derive(Debug, Default)]
pub(crate) struct PresentationSceneState {
    pending_binding: Option<LocalVisualBinding>,
    pending_viewport: Option<PresentationViewport>,
    captured_binding: Option<LocalVisualBinding>,
    captured_viewport: Option<PresentationViewport>,
    output: Option<SceneOutput>,
}

impl PresentationSceneState {
    pub(crate) fn set_binding(&mut self, binding: Option<LocalVisualBinding>) {
        self.pending_binding = binding;
    }

    pub(crate) fn set_viewport(&mut self, viewport: Option<PresentationViewport>) {
        self.pending_viewport = viewport;
    }

    pub(crate) fn begin_frame(&mut self) {
        self.captured_binding = None;
        self.captured_viewport = None;
        self.output = None;
    }

    pub(crate) fn capture(&mut self) {
        self.captured_binding = self.pending_binding;
        self.captured_viewport = self.pending_viewport;
    }

    pub(crate) fn resolve(
        &mut self,
        movement: Option<MovementProxy>,
    ) -> Result<(), PresentationSceneError> {
        let (Some(binding), Some(viewport), Some(movement)) =
            (self.captured_binding, self.captured_viewport, movement)
        else {
            self.output = None;
            return Ok(());
        };

        let position = vector_to_f32(movement.visual_position_meters())?;
        let orientation = quaternion_to_f32(movement.visual_orientation())?;
        let transform = Mat4::from_rotation_translation(orientation, position);
        let camera_rotation = orientation;
        let eye = position
            + camera_rotation
                * Vec3::new(0.0, CAMERA_VERTICAL_OFFSET_METERS, CAMERA_DISTANCE_METERS);
        let target = position + camera_rotation * Vec3::new(0.0, 1.0, -CAMERA_LOOK_AHEAD_METERS);
        let view = view::look_at_mat4(eye, target, camera_rotation * Vec3::Y);
        let aspect = viewport_aspect(viewport);
        let projection = directx::perspective(
            VERTICAL_FIELD_OF_VIEW_RADIANS,
            aspect,
            NEAR_PLANE_METERS,
            FAR_PLANE_METERS,
        );
        self.output = Some(SceneOutput {
            view: RenderView {
                id: PRIMARY_VIEW_ID,
                view: view.to_cols_array(),
                projection: projection.to_cols_array(),
                viewport: [0, 0, viewport.width(), viewport.height()],
                layer_mask: binding.layer_mask(),
            },
            instance: RenderInstance {
                id: movement.source().get(),
                resource: binding.resource(),
                transform: transform.to_cols_array(),
                layer_mask: binding.layer_mask(),
            },
        });
        Ok(())
    }

    pub(crate) fn build_frame(&self, id: RenderFrameId) -> RenderFrame {
        let Some(output) = &self.output else {
            return RenderFrame::empty(id);
        };
        RenderFrame {
            id,
            views: vec![output.view.clone()],
            instances: vec![output.instance.clone()],
        }
    }

    pub(crate) fn release_captured(&mut self) {
        self.captured_binding = None;
        self.captured_viewport = None;
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the renderer contract intentionally projects finite f64 simulation coordinates to f32"
)]
fn vector_to_f32(value: DVec3) -> Result<Vec3, PresentationSceneError> {
    let converted = value.as_vec3();
    if !converted.is_finite() {
        return Err(PresentationSceneError::TransformOutOfRange);
    }
    Ok(converted)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the renderer contract intentionally projects normalized f64 orientation to f32"
)]
fn quaternion_to_f32(value: DQuat) -> Result<Quat, PresentationSceneError> {
    let quaternion = value.as_quat();
    if !quaternion.is_finite() || quaternion.length_squared() <= f32::EPSILON {
        return Err(PresentationSceneError::TransformOutOfRange);
    }
    Ok(quaternion.normalize())
}

#[allow(
    clippy::cast_precision_loss,
    reason = "physical pixel dimensions only need f32 precision when deriving an aspect ratio"
)]
fn viewport_aspect(viewport: PresentationViewport) -> f32 {
    viewport.width() as f32 / viewport.height() as f32
}
