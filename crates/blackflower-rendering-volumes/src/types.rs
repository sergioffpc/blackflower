use glam::{IVec3, Vec3};

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
pub type WorldBounds = Bounds3<Vec3>;

/// Value encoding stored by a VDB grid.
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
}

/// Semantic class assigned to a VDB grid.
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

/// Immutable metadata copied from one VDB grid.
#[derive(Debug, Clone, PartialEq)]
pub struct GridMetadata {
    pub(crate) name: String,
    pub(crate) grid_type: GridType,
    pub(crate) grid_class: GridClass,
    pub(crate) byte_size: u64,
    pub(crate) active_voxel_count: u64,
    pub(crate) index_bounds: Option<IndexBounds>,
    pub(crate) world_bounds: Option<WorldBounds>,
    pub(crate) voxel_size: Vec3,
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
    pub const fn voxel_size(&self) -> Vec3 {
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
