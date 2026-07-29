use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use glam::Vec3A;

use crate::error::Error;
use crate::ffi::{self, Status};
use crate::query::{Query, QueryBuilder};

static NEXT_NAVMESH_KEY: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct NavMeshKey(u64);

/// A polygon reference scoped to its owning navigation mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PolygonRef {
    pub(crate) raw: u32,
    pub(crate) navmesh: NavMeshKey,
}

impl PolygonRef {
    /// Return Detour's opaque polygon reference value.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.raw
    }
}

/// A tile reference scoped to its owning navigation mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileRef {
    pub(crate) raw: u32,
    pub(crate) navmesh: NavMeshKey,
}

impl TileRef {
    /// Return Detour's opaque tile reference value.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.raw
    }
}

/// Validated parameters used to initialize a tiled Detour navigation mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavMeshParams {
    pub(crate) origin: Vec3A,
    pub(crate) tile_width: f32,
    pub(crate) tile_height: f32,
    pub(crate) max_tiles: NonZeroU32,
    pub(crate) max_polygons_per_tile: NonZeroU32,
}

impl NavMeshParams {
    /// Construct parameters matching the asset cooker's tiled mesh layout.
    pub fn new(
        origin: Vec3A,
        tile_width: f32,
        tile_height: f32,
        max_tiles: NonZeroU32,
        max_polygons_per_tile: NonZeroU32,
    ) -> Result<Self, Error> {
        if !origin.is_finite()
            || !tile_width.is_finite()
            || !tile_height.is_finite()
            || tile_width <= 0.0
            || tile_height <= 0.0
            || i32::try_from(max_tiles.get()).is_err()
            || i32::try_from(max_polygons_per_tile.get()).is_err()
        {
            return Err(Error::InvalidNavMeshParameters);
        }
        Ok(Self {
            origin,
            tile_width,
            tile_height,
            max_tiles,
            max_polygons_per_tile,
        })
    }
}

/// An owning Detour runtime navigation mesh.
///
/// Tile bytes are copied into Detour-owned memory. `NavMesh` is deliberately
/// neither `Send` nor `Sync` until cross-thread mutation and allocator rules
/// are validated for Blackflower's runtime scheduling model.
pub struct NavMesh {
    pub(crate) pointer: ffi::NavMeshPtr,
    pub(crate) key: NavMeshKey,
    not_send_sync: PhantomData<Rc<()>>,
}

impl NavMesh {
    /// Load one standalone tile produced by Detour's `dtCreateNavMeshData`.
    pub fn from_tile_data(data: &[u8]) -> Result<Self, Error> {
        let pointer = ffi::create_single_tile(data).map_err(map_single_tile_status)?;
        Ok(Self::from_pointer(pointer))
    }

    /// Initialize an empty tiled navigation mesh.
    pub fn tiled(params: NavMeshParams) -> Result<Self, Error> {
        let pointer = ffi::create_tiled(params).map_err(map_tiled_status)?;
        Ok(Self::from_pointer(pointer))
    }

    /// Copy and add one cooked tile to a tiled navigation mesh.
    pub fn add_tile(&mut self, data: &[u8]) -> Result<TileRef, Error> {
        let raw = ffi::add_tile(self.pointer, data).map_err(map_add_tile_status)?;
        Ok(TileRef {
            raw,
            navmesh: self.key,
        })
    }

    /// Remove a cooked tile from this tiled navigation mesh.
    pub fn remove_tile(&mut self, tile: TileRef) -> Result<(), Error> {
        self.validate_tile_owner(tile)?;
        ffi::remove_tile(self.pointer, tile.raw).map_err(map_remove_tile_status)
    }

    /// Replace a cooked tile while preserving its stable Detour reference.
    pub fn replace_tile(&mut self, tile: TileRef, data: &[u8]) -> Result<TileRef, Error> {
        self.validate_tile_owner(tile)?;
        let raw =
            ffi::replace_tile(self.pointer, tile.raw, data).map_err(map_replace_tile_status)?;
        Ok(TileRef {
            raw,
            navmesh: self.key,
        })
    }

    /// Test whether a tile reference was created by this navigation mesh.
    #[must_use]
    pub const fn owns_tile(&self, tile: TileRef) -> bool {
        self.key.0 == tile.navmesh.0
    }

    /// Create a query with the default capacities.
    pub fn query(&self) -> Result<Query<'_>, Error> {
        QueryBuilder::new(self).build()
    }

    /// Configure a query's node and result capacities.
    #[must_use]
    pub const fn query_builder(&self) -> QueryBuilder<'_> {
        QueryBuilder::new(self)
    }

    fn validate_tile_owner(&self, tile: TileRef) -> Result<(), Error> {
        if self.owns_tile(tile) {
            Ok(())
        } else {
            Err(Error::WrongNavMesh)
        }
    }

    fn from_pointer(pointer: ffi::NavMeshPtr) -> Self {
        Self {
            pointer,
            key: NavMeshKey(NEXT_NAVMESH_KEY.fetch_add(1, Ordering::Relaxed)),
            not_send_sync: PhantomData,
        }
    }
}

impl Drop for NavMesh {
    fn drop(&mut self) {
        ffi::destroy_navmesh(self.pointer);
    }
}

const fn map_single_tile_status(status: Status) -> Error {
    match status {
        Status::InvalidNavMeshData | Status::InvalidArgument => Error::InvalidNavMeshData,
        Status::OutOfMemory => Error::AllocationFailed,
        Status::InitializationFailed => Error::NavMeshInitialization,
        Status::TileAlreadyOccupied | Status::QueryFailed | Status::ContractViolation => {
            Error::NativeContract
        }
    }
}

const fn map_tiled_status(status: Status) -> Error {
    match status {
        Status::InvalidArgument => Error::InvalidNavMeshParameters,
        Status::OutOfMemory => Error::AllocationFailed,
        Status::InitializationFailed => Error::NavMeshInitialization,
        Status::InvalidNavMeshData
        | Status::TileAlreadyOccupied
        | Status::QueryFailed
        | Status::ContractViolation => Error::NativeContract,
    }
}

const fn map_add_tile_status(status: Status) -> Error {
    match status {
        Status::InvalidNavMeshData | Status::InvalidArgument => Error::InvalidNavMeshData,
        Status::OutOfMemory => Error::TileCapacityExhausted,
        Status::TileAlreadyOccupied => Error::TileAlreadyOccupied,
        Status::InitializationFailed | Status::QueryFailed | Status::ContractViolation => {
            Error::NativeContract
        }
    }
}

const fn map_remove_tile_status(status: Status) -> Error {
    match status {
        Status::InvalidArgument => Error::InvalidTile,
        Status::OutOfMemory
        | Status::InvalidNavMeshData
        | Status::InitializationFailed
        | Status::TileAlreadyOccupied
        | Status::QueryFailed
        | Status::ContractViolation => Error::NativeContract,
    }
}

const fn map_replace_tile_status(status: Status) -> Error {
    match status {
        Status::InvalidNavMeshData => Error::InvalidNavMeshData,
        Status::InvalidArgument => Error::InvalidTile,
        Status::OutOfMemory => Error::AllocationFailed,
        Status::InitializationFailed
        | Status::TileAlreadyOccupied
        | Status::QueryFailed
        | Status::ContractViolation => Error::NativeContract,
    }
}
