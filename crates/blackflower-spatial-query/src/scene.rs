use std::sync::Arc;

use glam::Vec3A;

use crate::ffi;
use crate::{Error, GeometryId, SurfaceHit, Triangle};

struct DeviceInner {
    pointer: ffi::DevicePtr,
}

impl Drop for DeviceInner {
    fn drop(&mut self) {
        ffi::destroy_device(self.pointer);
    }
}

/// Shared Embree device used to build immutable query scenes.
#[derive(Clone)]
pub struct Device {
    inner: Arc<DeviceInner>,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Device").finish_non_exhaustive()
    }
}

impl Device {
    /// Create one Embree device using Blackflower's pinned configuration.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            inner: Arc::new(DeviceInner {
                pointer: ffi::create_device()?,
            }),
        })
    }

    /// Start building one immutable scene.
    pub fn create_scene(&self) -> Result<SceneBuilder, Error> {
        Ok(SceneBuilder {
            device: self.clone(),
            pointer: Some(ffi::create_scene(self.inner.pointer)?),
        })
    }
}

/// Mutable construction phase for one spatial query scene.
pub struct SceneBuilder {
    device: Device,
    pointer: Option<ffi::ScenePtr>,
}

impl std::fmt::Debug for SceneBuilder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SceneBuilder")
            .finish_non_exhaustive()
    }
}

impl SceneBuilder {
    /// Copy one triangle geometry into the scene and return its scene-local ID.
    pub fn add_triangles(&mut self, triangles: &[Triangle]) -> Result<GeometryId, Error> {
        if triangles.is_empty() {
            return Err(Error::EmptyGeometry);
        }
        let pointer = self.pointer.ok_or(Error::SceneCommitted)?;
        ffi::add_triangles(pointer, triangles).map(GeometryId)
    }

    /// Build Embree's acceleration structure and seal the scene for queries.
    pub fn commit(mut self) -> Result<Scene, Error> {
        let pointer = self.pointer.ok_or(Error::SceneCommitted)?;
        ffi::commit_scene(pointer)?;
        let pointer = self.pointer.take().ok_or(Error::ContractViolation)?;
        Ok(Scene {
            _device: self.device.clone(),
            pointer,
        })
    }
}

impl Drop for SceneBuilder {
    fn drop(&mut self) {
        if let Some(pointer) = self.pointer.take() {
            ffi::destroy_scene(pointer);
        }
    }
}

/// Immutable committed Embree scene supporting concurrent read-only queries.
pub struct Scene {
    _device: Device,
    pointer: ffi::ScenePtr,
}

impl std::fmt::Debug for Scene {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Scene").finish_non_exhaustive()
    }
}

impl Scene {
    /// Return the nearest bounded set of unique surface crossings for a segment.
    pub fn intersect_segment(
        &self,
        start: Vec3A,
        end: Vec3A,
        max_hits: usize,
        output: &mut Vec<SurfaceHit>,
    ) -> Result<(), Error> {
        if !start.is_finite() || !end.is_finite() {
            output.clear();
            return Err(Error::ContractViolation);
        }
        ffi::intersect_segment(self.pointer, start, end, max_hits, output)
    }

    /// Return the closest surface crossing, if any.
    pub fn closest_hit(&self, start: Vec3A, end: Vec3A) -> Result<Option<SurfaceHit>, Error> {
        if !start.is_finite() || !end.is_finite() {
            return Err(Error::ContractViolation);
        }
        ffi::closest_hit(self.pointer, start, end)
    }

    /// Whether any surface crosses the segment.
    pub fn is_occluded(&self, start: Vec3A, end: Vec3A) -> Result<bool, Error> {
        if !start.is_finite() || !end.is_finite() {
            return Err(Error::ContractViolation);
        }
        ffi::is_occluded(self.pointer, start, end)
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        ffi::destroy_scene(self.pointer);
    }
}
