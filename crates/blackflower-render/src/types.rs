use glam::{DVec3, IVec3};

/// Inclusive axis-aligned bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds3<T> {
    min: T,
    max: T,
}

impl<T> Bounds3<T> {
    pub(crate) const fn new(min: T, max: T) -> Self {
        Self { min, max }
    }

    /// Return the lower corner.
    #[must_use]
    pub const fn min(&self) -> &T {
        &self.min
    }

    /// Return the upper corner.
    #[must_use]
    pub const fn max(&self) -> &T {
        &self.max
    }
}

/// Inclusive active-voxel bounds in index space.
pub type IndexBounds = Bounds3<IVec3>;

/// Active-value bounds in world space.
pub type WorldBounds = Bounds3<DVec3>;

/// Value encoding stored by a NanoVDB grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GridType {
    Unknown,
    Float,
    Double,
    Int16,
    Int32,
    Int64,
    Vec3f,
    Vec3d,
    Mask,
    Half,
    UInt32,
    Boolean,
    Rgba8,
    Fp4,
    Fp8,
    Fp16,
    FpN,
    Vec4f,
    Vec4d,
    Index,
    OnIndex,
    PointIndex,
    Vec3u8,
    Vec3u16,
    UInt8,
}

impl GridType {
    /// Return whether this encoding can be sampled through [`crate::FloatGrid`].
    #[must_use]
    pub const fn is_float(self) -> bool {
        matches!(
            self,
            Self::Float | Self::Fp4 | Self::Fp8 | Self::Fp16 | Self::FpN
        )
    }

    pub(crate) const fn from_raw(value: u32) -> Option<Self> {
        Some(match value {
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_UNKNOWN => Self::Unknown,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_FLOAT => Self::Float,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_DOUBLE => Self::Double,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_INT16 => Self::Int16,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_INT32 => Self::Int32,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_INT64 => Self::Int64,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_VEC3F => Self::Vec3f,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_VEC3D => Self::Vec3d,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_MASK => Self::Mask,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_HALF => Self::Half,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_UINT32 => Self::UInt32,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_BOOLEAN => Self::Boolean,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_RGBA8 => Self::Rgba8,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_FP4 => Self::Fp4,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_FP8 => Self::Fp8,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_FP16 => Self::Fp16,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_FPN => Self::FpN,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_VEC4F => Self::Vec4f,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_VEC4D => Self::Vec4d,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_INDEX => Self::Index,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_ON_INDEX => Self::OnIndex,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_POINT_INDEX => Self::PointIndex,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_VEC3U8 => Self::Vec3u8,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_VEC3U16 => Self::Vec3u16,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_TYPE_UINT8 => Self::UInt8,
            _ => return None,
        })
    }
}

/// Semantic class assigned to a NanoVDB grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GridClass {
    Unknown,
    LevelSet,
    FogVolume,
    Staggered,
    PointIndex,
    PointData,
    Topology,
    VoxelVolume,
    IndexGrid,
    TensorGrid,
}

impl GridClass {
    pub(crate) const fn from_raw(value: u32) -> Option<Self> {
        Some(match value {
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_CLASS_UNKNOWN => Self::Unknown,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_CLASS_LEVEL_SET => Self::LevelSet,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_CLASS_FOG_VOLUME => Self::FogVolume,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_CLASS_STAGGERED => Self::Staggered,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_CLASS_POINT_INDEX => Self::PointIndex,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_CLASS_POINT_DATA => Self::PointData,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_CLASS_TOPOLOGY => Self::Topology,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_CLASS_VOXEL_VOLUME => Self::VoxelVolume,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_CLASS_INDEX_GRID => Self::IndexGrid,
            crate::ffi::raw::BF_RENDER_NANOVDB_GRID_CLASS_TENSOR_GRID => Self::TensorGrid,
            _ => return None,
        })
    }
}

/// Immutable metadata copied from one NanoVDB grid.
#[derive(Debug, Clone, PartialEq)]
pub struct GridMetadata {
    pub(crate) name: String,
    pub(crate) grid_type: GridType,
    pub(crate) grid_class: GridClass,
    pub(crate) byte_size: u64,
    pub(crate) active_voxel_count: u64,
    pub(crate) index_bounds: Option<IndexBounds>,
    pub(crate) world_bounds: Option<WorldBounds>,
    pub(crate) voxel_size: DVec3,
}

impl GridMetadata {
    /// Return the grid name stored in the asset.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the stored value encoding.
    #[must_use]
    pub const fn grid_type(&self) -> GridType {
        self.grid_type
    }

    /// Return the grid's semantic class.
    #[must_use]
    pub const fn grid_class(&self) -> GridClass {
        self.grid_class
    }

    /// Return the serialized grid size in bytes.
    #[must_use]
    pub const fn byte_size(&self) -> u64 {
        self.byte_size
    }

    /// Return the number of active voxels.
    #[must_use]
    pub const fn active_voxel_count(&self) -> u64 {
        self.active_voxel_count
    }

    /// Return the active index-space bounds, or `None` for an empty grid.
    #[must_use]
    pub const fn index_bounds(&self) -> Option<IndexBounds> {
        self.index_bounds
    }

    /// Return the active world-space bounds, or `None` for an empty grid.
    #[must_use]
    pub const fn world_bounds(&self) -> Option<WorldBounds> {
        self.world_bounds
    }

    /// Return the voxel dimensions in world units.
    #[must_use]
    pub const fn voxel_size(&self) -> DVec3 {
        self.voxel_size
    }
}

/// One scalar voxel value and its active state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatVoxel {
    value: f32,
    active: bool,
}

impl FloatVoxel {
    pub(crate) const fn new(value: f32, active: bool) -> Self {
        Self { value, active }
    }

    /// Return the voxel value, including a grid's background value when inactive.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.value
    }

    /// Return whether the voxel is active.
    #[must_use]
    pub const fn is_active(self) -> bool {
        self.active
    }
}
