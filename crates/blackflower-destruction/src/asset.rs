use std::marker::PhantomData;
use std::rc::Rc;

use crate::ffi;
use crate::{BondDesc, ChunkDesc, Error};

/// Immutable Blast chunk hierarchy and support-bond graph.
pub struct Asset {
    pub(crate) pointer: ffi::AssetPointer,
    pub(crate) chunk_count: u32,
    pub(crate) bond_count: u32,
    pub(crate) graph_node_count: u32,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl Asset {
    /// Builds an immutable Blast asset from validated authored descriptors.
    pub fn new(chunks: &[ChunkDesc], bonds: &[BondDesc]) -> Result<Self, Error> {
        validate_chunks(chunks)?;
        validate_bonds(chunks.len(), bonds)?;
        let native_chunks = chunks
            .iter()
            .copied()
            .map(ffi::NativeChunk::from)
            .collect::<Vec<_>>();
        let native_bonds = bonds
            .iter()
            .copied()
            .map(ffi::NativeBond::from)
            .collect::<Vec<_>>();
        let pointer = ffi::create_asset(&native_chunks, &native_bonds)?;
        Ok(Self {
            pointer,
            chunk_count: ffi::asset_chunk_count(pointer),
            bond_count: ffi::asset_bond_count(pointer),
            graph_node_count: ffi::asset_graph_node_count(pointer),
            not_send_or_sync: PhantomData,
        })
    }

    /// Returns the number of authored chunks retained by Blast.
    #[must_use]
    pub const fn chunk_count(&self) -> u32 {
        self.chunk_count
    }

    /// Returns the number of unique valid bonds retained by Blast.
    #[must_use]
    pub const fn bond_count(&self) -> u32 {
        self.bond_count
    }

    /// Returns the number of support chunks in the graph.
    #[must_use]
    pub fn support_chunk_count(&self) -> u32 {
        ffi::asset_support_chunk_count(self.pointer)
    }

    /// Returns support-graph nodes, including the optional external-world node.
    #[must_use]
    pub const fn graph_node_count(&self) -> u32 {
        self.graph_node_count
    }
}

impl Drop for Asset {
    fn drop(&mut self) {
        ffi::destroy_asset(self.pointer);
    }
}

fn validate_chunks(chunks: &[ChunkDesc]) -> Result<(), Error> {
    if chunks.is_empty() || chunks.len() > u32::MAX as usize {
        return Err(Error::InvalidChunk);
    }
    for (index, chunk) in chunks.iter().enumerate() {
        if !chunk.centroid.is_finite() || !chunk.volume.is_finite() || chunk.volume <= 0.0 {
            return Err(Error::InvalidChunk);
        }
        if chunk
            .parent
            .is_some_and(|parent| usize::try_from(parent).map_or(true, |parent| parent >= index))
        {
            return Err(Error::InvalidChunk);
        }
    }
    Ok(())
}

fn validate_bonds(chunk_count: usize, bonds: &[BondDesc]) -> Result<(), Error> {
    if bonds.len() > u32::MAX as usize {
        return Err(Error::InvalidBond);
    }
    for bond in bonds {
        let normal_length_squared = bond.normal.length_squared();
        if !bond.normal.is_finite()
            || !normal_length_squared.is_finite()
            || normal_length_squared <= f32::EPSILON
            || !bond.centroid.is_finite()
            || !bond.area.is_finite()
            || bond.area <= 0.0
            || bond.chunks == [None, None]
            || bond.chunks[0].is_some() && bond.chunks[0] == bond.chunks[1]
            || bond
                .chunks
                .iter()
                .flatten()
                .any(|&chunk| usize::try_from(chunk).map_or(true, |chunk| chunk >= chunk_count))
        {
            return Err(Error::InvalidBond);
        }
    }
    Ok(())
}
