use std::rc::Rc;
use std::sync::Arc;

use glam::{Mat4, Vec3A};

use crate::RayTracerBackend;
use crate::error::Error;
use crate::ffi;
use crate::hrtf::ContextInner;

/// Frequency-dependent acoustic response of one scene surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcousticMaterial {
    absorption: [f32; 3],
    scattering: f32,
    transmission: [f32; 3],
}

impl AcousticMaterial {
    /// Create a material whose coefficients are finite fractions from zero to one.
    pub fn new(
        absorption: [f32; 3],
        scattering: f32,
        transmission: [f32; 3],
    ) -> Result<Self, Error> {
        if absorption
            .into_iter()
            .chain([scattering])
            .chain(transmission)
            .all(|coefficient| coefficient.is_finite() && (0.0..=1.0).contains(&coefficient))
        {
            Ok(Self {
                absorption,
                scattering,
                transmission,
            })
        } else {
            Err(Error::InvalidAcousticMaterial)
        }
    }

    pub(crate) const fn absorption(self) -> [f32; 3] {
        self.absorption
    }

    pub(crate) const fn scattering(self) -> f32 {
        self.scattering
    }

    pub(crate) const fn transmission(self) -> [f32; 3] {
        self.transmission
    }
}

/// Counter-clockwise triangle indexing three vertices in an acoustic mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcousticTriangle([u32; 3]);

impl AcousticTriangle {
    /// Create a triangle from three vertex indices.
    #[must_use]
    pub const fn new(first: u32, second: u32, third: u32) -> Self {
        Self([first, second, third])
    }

    /// Return the three vertex indices.
    #[must_use]
    pub const fn indices(self) -> [u32; 3] {
        self.0
    }
}

pub(crate) struct SceneInner {
    pub(crate) context: Arc<ContextInner>,
    pub(crate) pointer: ffi::ScenePtr,
    backend: RayTracerBackend,
}

impl Drop for SceneInner {
    fn drop(&mut self) {
        ffi::destroy_scene(self.pointer);
    }
}

/// Mutable Steam Audio scene containing committed acoustic geometry.
pub struct Scene {
    inner: Rc<SceneInner>,
}

impl Scene {
    pub(crate) fn new(
        context: Arc<ContextInner>,
        backend: RayTracerBackend,
    ) -> Result<Self, Error> {
        let embree = match backend {
            RayTracerBackend::BuiltIn => None,
            RayTracerBackend::Embree => Some(
                context
                    .embree_device()
                    .ok_or(Error::RayTracerUnavailable { backend })?,
            ),
        };
        let pointer = ffi::create_scene(context.pointer, embree)
            .map_err(|status| Error::from_status("iplSceneCreate", status))?;
        Ok(Self {
            inner: Rc::new(SceneInner {
                context,
                pointer,
                backend,
            }),
        })
    }

    pub(crate) fn from_serialized(
        context: Arc<ContextInner>,
        serialized: &[u8],
    ) -> Result<Self, Error> {
        let pointer = ffi::load_scene(context.pointer, context.embree_device(), serialized)
            .map_err(|status| Error::from_status("iplSceneLoad", status))?;
        let backend = context.ray_tracer_backend();
        Ok(Self {
            inner: Rc::new(SceneInner {
                context,
                pointer,
                backend,
            }),
        })
    }

    /// Ray tracer used by this scene.
    #[must_use]
    pub fn ray_tracer_backend(&self) -> RayTracerBackend {
        self.inner.backend
    }

    /// Serialize the committed scene into a validated `.bfacscn` asset.
    pub fn to_acoustic_asset(
        &self,
        vertex_count: u32,
        triangle_count: u32,
        material_count: u32,
    ) -> Result<crate::AcousticScene, Error> {
        if self.inner.backend != RayTracerBackend::BuiltIn {
            return Err(Error::SceneSerializationRequiresBuiltIn);
        }
        let serialized = ffi::save_scene(self.inner.context.pointer, self.inner.pointer)
            .map_err(|status| Error::from_status("iplSceneSave", status))?;
        crate::AcousticScene::encode(serialized, vertex_count, triangle_count, material_count)
    }

    pub(crate) fn inner(&self) -> &Rc<SceneInner> {
        &self.inner
    }

    /// Create an acoustic triangle mesh owned by this scene.
    pub fn create_static_mesh(
        &mut self,
        vertices: &[Vec3A],
        triangles: &[AcousticTriangle],
        material_indices: &[u32],
        materials: &[AcousticMaterial],
    ) -> Result<StaticMesh, Error> {
        let (triangles, material_indices) =
            validate_geometry(vertices, triangles, material_indices, materials)?;
        let pointer = ffi::create_static_mesh(
            self.inner.pointer,
            vertices,
            &triangles,
            &material_indices,
            materials,
        )
        .map_err(|status| Error::from_status("iplStaticMeshCreate", status))?;
        Ok(StaticMesh {
            scene: Rc::clone(&self.inner),
            pointer,
            added: false,
        })
    }

    /// Create one rigid instance of a committed sub-scene.
    pub fn create_instanced_mesh(
        &mut self,
        sub_scene: &Scene,
        transform: Mat4,
    ) -> Result<InstancedMesh, Error> {
        if !Arc::ptr_eq(&self.inner.context, &sub_scene.inner.context) {
            return Err(Error::WrongAcousticContext);
        }
        validate_transform(transform)?;
        let pointer =
            ffi::create_instanced_mesh(self.inner.pointer, sub_scene.inner.pointer, transform)
                .map_err(|status| Error::from_status("iplInstancedMeshCreate", status))?;
        Ok(InstancedMesh {
            scene: Rc::clone(&self.inner),
            sub_scene: Rc::clone(&sub_scene.inner),
            pointer,
            added: false,
        })
    }

    /// Publish all pending mesh additions and removals to acoustic queries.
    pub fn commit(&mut self) {
        ffi::commit_scene(self.inner.pointer);
    }
}

/// Acoustic triangle mesh created for one [`Scene`].
pub struct StaticMesh {
    scene: Rc<SceneInner>,
    pointer: ffi::StaticMeshPtr,
    added: bool,
}

impl StaticMesh {
    /// Add this mesh to its scene.
    pub fn add(&mut self) {
        if !self.added {
            ffi::add_static_mesh(self.scene.pointer, self.pointer);
            self.added = true;
        }
    }

    /// Remove this mesh from its scene.
    pub fn remove(&mut self) {
        if self.added {
            ffi::remove_static_mesh(self.scene.pointer, self.pointer);
            self.added = false;
        }
    }

    /// Whether this mesh is included in its scene's next committed geometry.
    #[must_use]
    pub const fn is_added(&self) -> bool {
        self.added
    }
}

impl Drop for StaticMesh {
    fn drop(&mut self) {
        self.remove();
        ffi::destroy_static_mesh(self.pointer);
    }
}

/// Rigid Steam Audio sub-scene instance updated and committed off the callback.
pub struct InstancedMesh {
    scene: Rc<SceneInner>,
    sub_scene: Rc<SceneInner>,
    pointer: ffi::InstancedMeshPtr,
    added: bool,
}

impl InstancedMesh {
    /// Add this instance to its parent scene.
    pub fn add(&mut self) {
        if !self.added {
            ffi::add_instanced_mesh(self.scene.pointer, self.pointer);
            self.added = true;
        }
    }

    /// Remove this instance from its parent scene.
    pub fn remove(&mut self) {
        if self.added {
            ffi::remove_instanced_mesh(self.scene.pointer, self.pointer);
            self.added = false;
        }
    }

    /// Coalesce a new rigid transform before the parent scene's next commit.
    pub fn update_transform(&mut self, transform: Mat4) -> Result<(), Error> {
        validate_transform(transform)?;
        ffi::update_instanced_mesh_transform(self.scene.pointer, self.pointer, transform);
        Ok(())
    }

    /// Whether this instance is pending or present in its parent scene.
    #[must_use]
    pub const fn is_added(&self) -> bool {
        self.added
    }
}

impl Drop for InstancedMesh {
    fn drop(&mut self) {
        self.remove();
        ffi::destroy_instanced_mesh(self.pointer);
        let _keep_sub_scene_alive = &self.sub_scene;
    }
}

fn validate_transform(transform: Mat4) -> Result<(), Error> {
    if transform.is_finite() {
        Ok(())
    } else {
        Err(Error::InvalidSceneGeometry)
    }
}

fn validate_geometry(
    vertices: &[Vec3A],
    triangles: &[AcousticTriangle],
    material_indices: &[u32],
    materials: &[AcousticMaterial],
) -> Result<(Vec<[i32; 3]>, Vec<i32>), Error> {
    validate_geometry_lengths(vertices, triangles, material_indices, materials)?;
    if vertices.iter().any(|vertex| !vertex.is_finite()) {
        return Err(Error::InvalidSceneGeometry);
    }
    let vertex_count = vertices.len();
    let triangles = triangles
        .iter()
        .copied()
        .map(AcousticTriangle::indices)
        .map(|indices| validate_triangle(indices, vertex_count))
        .collect::<Result<Vec<_>, _>>()?;
    let material_count = materials.len();
    let material_indices = material_indices
        .iter()
        .copied()
        .map(|index| validate_material_index(index, material_count))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((triangles, material_indices))
}

fn validate_geometry_lengths(
    vertices: &[Vec3A],
    triangles: &[AcousticTriangle],
    material_indices: &[u32],
    materials: &[AcousticMaterial],
) -> Result<(), Error> {
    if vertices.is_empty()
        || triangles.is_empty()
        || materials.is_empty()
        || triangles.len() != material_indices.len()
    {
        return Err(Error::InvalidSceneGeometry);
    }
    if [vertices.len(), triangles.len(), materials.len()]
        .into_iter()
        .any(|len| i32::try_from(len).is_err())
    {
        return Err(Error::SceneGeometryCountOutOfRange);
    }
    Ok(())
}

fn validate_triangle(indices: [u32; 3], vertex_count: usize) -> Result<[i32; 3], Error> {
    if indices[0] == indices[1]
        || indices[0] == indices[2]
        || indices[1] == indices[2]
        || indices
            .into_iter()
            .any(|index| usize::try_from(index).map_or(true, |index| index >= vertex_count))
    {
        return Err(Error::InvalidSceneGeometry);
    }
    let indices = indices.map(|index| {
        i32::try_from(index).unwrap_or_else(|_error| {
            unreachable!("validated vertex indices must fit the native vertex count")
        })
    });
    Ok(indices)
}

fn validate_material_index(index: u32, material_count: usize) -> Result<i32, Error> {
    let index = usize::try_from(index).map_err(|_error| Error::InvalidSceneGeometry)?;
    if index >= material_count {
        return Err(Error::InvalidSceneGeometry);
    }
    i32::try_from(index).map_err(|_error| Error::InvalidSceneGeometry)
}
