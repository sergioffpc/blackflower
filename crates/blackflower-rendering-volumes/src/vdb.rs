use std::iter::FusedIterator;

use glam::{IVec3, Vec3};

use crate::error::Error;
use crate::ffi::{self, Status};
use crate::types::{FloatVoxel, GridMetadata};

/// Immutable ownership of one or more VDB grids.
#[derive(Debug)]
pub struct Vdb {
    pointer: ffi::HandlePtr,
    metadata: Box<[GridMetadata]>,
}

impl Vdb {
    /// Load all grids from trusted uncompressed `.nvdb` or raw VDB bytes.
    ///
    /// VDB is a runtime asset format, not a sandboxed interchange format.
    /// Only load content produced by the matching trusted content pipeline.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let pointer = ffi::load(bytes).map_err(map_load)?;
        let grid_count =
            usize::try_from(ffi::grid_count(pointer)).map_err(|_error| Error::NativeContract)?;
        let metadata = (0..grid_count)
            .map(|index| {
                let index = u32::try_from(index).map_err(|_error| Error::NativeContract)?;
                ffi::grid_metadata(pointer, index).map_err(map_metadata)
            })
            .collect::<Result<Vec<_>, _>>();

        match metadata {
            Ok(metadata) => Ok(Self {
                pointer,
                metadata: metadata.into_boxed_slice(),
            }),
            Err(error) => {
                ffi::destroy(pointer);
                Err(error)
            }
        }
    }

    /// Return the number of grids stored in this asset.
    #[must_use]
    pub fn len(&self) -> usize {
        self.metadata.len()
    }

    /// Return whether this asset contains no grids.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }

    /// Return a grid by zero-based index.
    #[must_use]
    pub fn grid(&self, index: usize) -> Option<Grid<'_>> {
        let metadata = self.metadata.get(index)?;
        let index = u32::try_from(index).ok()?;
        Some(Grid {
            owner: self,
            index,
            metadata,
        })
    }

    /// Return the uniquely named grid, if present.
    #[must_use]
    pub fn grid_by_name(&self, name: &str) -> Option<Grid<'_>> {
        let mut matches = self
            .metadata
            .iter()
            .enumerate()
            .filter(|(_index, metadata)| metadata.name() == name);
        let (index, _metadata) = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        self.grid(index)
    }

    /// Iterate over every grid in asset order.
    pub fn grids(&self) -> GridIter<'_> {
        GridIter {
            owner: self,
            indices: 0..self.len(),
        }
    }
}

impl Drop for Vdb {
    fn drop(&mut self) {
        ffi::destroy(self.pointer);
    }
}

/// A borrowed immutable VDB grid.
#[derive(Debug, Clone, Copy)]
pub struct Grid<'a> {
    owner: &'a Vdb,
    index: u32,
    metadata: &'a GridMetadata,
}

impl<'a> Grid<'a> {
    /// Return metadata copied while the asset was loaded.
    #[must_use]
    pub const fn metadata(self) -> &'a GridMetadata {
        self.metadata
    }

    /// View this grid as a scalar floating-point grid when its encoding is supported.
    #[must_use]
    pub const fn as_float(self) -> Option<FloatGrid<'a>> {
        if self.metadata.grid_type.is_float() {
            Some(FloatGrid { grid: self })
        } else {
            None
        }
    }

    /// Transform a finite index-space position into world space.
    pub fn index_to_world(self, position: Vec3) -> Result<Vec3, Error> {
        validate_position(position)?;
        ffi::index_to_world(self.owner.pointer, self.index, position).map_err(map_runtime)
    }

    /// Transform a finite world-space position into index space.
    pub fn world_to_index(self, position: Vec3) -> Result<Vec3, Error> {
        validate_position(position)?;
        ffi::world_to_index(self.owner.pointer, self.index, position).map_err(map_runtime)
    }
}

/// A borrowed Float, Fp4, Fp8, Fp16, or FpN VDB grid.
#[derive(Debug, Clone, Copy)]
pub struct FloatGrid<'a> {
    grid: Grid<'a>,
}

impl<'a> FloatGrid<'a> {
    /// Return the underlying grid and metadata.
    #[must_use]
    pub const fn grid(self) -> Grid<'a> {
        self.grid
    }

    /// Read a voxel value and its active state at an integer index coordinate.
    pub fn voxel(self, coordinate: IVec3) -> Result<FloatVoxel, Error> {
        ffi::float_voxel(self.grid.owner.pointer, self.grid.index, coordinate).map_err(map_runtime)
    }

    /// Trilinearly sample a finite world-space position.
    pub fn sample_world(self, position: Vec3) -> Result<f32, Error> {
        validate_position(position)?;
        ffi::sample_float_world(self.grid.owner.pointer, self.grid.index, position)
            .map_err(map_runtime)
    }
}

/// Iterator over the grids in a [`Vdb`] asset.
#[derive(Debug, Clone)]
pub struct GridIter<'a> {
    owner: &'a Vdb,
    indices: std::ops::Range<usize>,
}

impl<'a> Iterator for GridIter<'a> {
    type Item = Grid<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.indices.next().and_then(|index| self.owner.grid(index))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.indices.size_hint()
    }
}

impl DoubleEndedIterator for GridIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.indices
            .next_back()
            .and_then(|index| self.owner.grid(index))
    }
}

impl ExactSizeIterator for GridIter<'_> {}
impl FusedIterator for GridIter<'_> {}

fn validate_position(position: Vec3) -> Result<(), Error> {
    if position.is_finite() {
        Ok(())
    } else {
        Err(Error::InvalidPosition)
    }
}

const fn map_load(status: Status) -> Error {
    match status {
        Status::InvalidArgument | Status::InvalidAsset => Error::InvalidAsset,
        Status::UnsupportedCompression => Error::UnsupportedCompression,
        Status::OutOfMemory => Error::OutOfMemory,
        Status::IndexOutOfRange
        | Status::TypeMismatch
        | Status::NativeFailure
        | Status::ContractViolation => Error::NativeContract,
    }
}

const fn map_metadata(status: Status) -> Error {
    match status {
        Status::InvalidAsset => Error::InvalidGridName,
        Status::OutOfMemory => Error::OutOfMemory,
        Status::InvalidArgument
        | Status::UnsupportedCompression
        | Status::IndexOutOfRange
        | Status::TypeMismatch
        | Status::NativeFailure
        | Status::ContractViolation => Error::NativeContract,
    }
}

const fn map_runtime(status: Status) -> Error {
    match status {
        Status::OutOfMemory => Error::OutOfMemory,
        Status::InvalidArgument
        | Status::InvalidAsset
        | Status::UnsupportedCompression
        | Status::IndexOutOfRange
        | Status::TypeMismatch
        | Status::NativeFailure
        | Status::ContractViolation => Error::NativeContract,
    }
}
