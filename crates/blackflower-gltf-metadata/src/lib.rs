#![doc = include_str!("../README.md")]

mod acoustics;
mod animation;
mod container;
mod error;
mod map;
mod navigation;
mod node;
mod validation;

use std::path::Path;

use serde_json::Value;

pub use acoustics::{
    AcousticGeometryClass, AcousticMaterialMetadata, AcousticNodeKind, AcousticNodeMetadata,
};
pub use animation::{
    AdditiveMetadata, AdditiveReference, AnimationMarker, AnimationMarkers, AnimationMetadata,
    MotionAxis, RootMotionMetadata, RootMotionReference,
};
pub use error::Error;
pub use map::{
    AcousticGeometryClass as MapAcousticGeometryClass, AcousticPortalMetadata,
    AcousticZoneMetadata, AssetInstanceMetadata, AudioEmitterMetadata, GeometryMetadata,
    GeometryNavigation, MAP_METADATA_SCHEMA, MapMaterialMetadata, MapMetadata,
    MapNavigationDirection, MapNodeMetadata, MapNodeRole, NavigationLinkMetadata,
    SpawnPointMetadata, TriggerVolumeMetadata,
};
pub use navigation::{NavigationDirection, NavigationMetadata, NavigationRole};
pub use node::NodeMetadata;
pub use validation::GLTF_VERSION;

/// Parsed glTF JSON retained for typed Blackflower metadata queries.
#[derive(Debug)]
pub struct Document {
    root: Value,
}

impl Document {
    /// Open and completely validate a glTF 2.0 JSON or GLB source file.
    ///
    /// This is the cooker entry point. It confines external resources to the
    /// source directory, validates and imports those resources with the pinned
    /// `gltf` crate, and enforces Blackflower's extension allowlist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| Error::ReadSource {
            path: path.to_path_buf(),
            source,
        })?;
        let root = parse_root(&bytes)?;
        validation::validate_root(&root)?;
        validation::validate_file(path, &root)?;
        navigation::validate_all(&root)?;
        acoustics::validate_all(&root)?;
        Ok(Self { root })
    }

    /// Parse an in-memory glTF 2.0 JSON or GLB document with `gltf` validation.
    ///
    /// This entry point cannot resolve adjacent resources because no source
    /// path exists. Domain cookers must use [`Self::open`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let root = parse_root(bytes)?;
        validation::validate_root(&root)?;
        validation::validate_bytes(bytes)?;
        navigation::validate_all(&root)?;
        acoustics::validate_all(&root)?;
        Ok(Self { root })
    }

    /// Extract markers authored on exactly one named glTF animation.
    ///
    /// An animation without `extras.blackflower` has an empty marker track.
    pub fn animation_markers(&self, animation: &str) -> Result<AnimationMarkers, Error> {
        self.animation_metadata(animation).map(Into::into)
    }

    /// Extract typed cooking and playback policy from one named animation.
    ///
    /// Missing Blackflower extras produce disabled defaults.
    pub fn animation_metadata(&self, animation: &str) -> Result<AnimationMetadata, Error> {
        animation::metadata(&self.root, animation)
    }

    /// Extract typed metadata authored on exactly one named glTF node.
    ///
    /// A node without `extras.blackflower` returns `None`.
    pub fn node_metadata(&self, node: &str) -> Result<Option<NodeMetadata>, Error> {
        node::metadata(&self.root, node)
    }

    /// Extract and validate typed schema-1 authoring data from one named map scene.
    ///
    /// Nodes are returned in stable glTF node-index order. All map-local IDs,
    /// cross-node references, role payloads, mesh requirements, and material
    /// mappings are validated before this method succeeds.
    pub fn map_metadata(&self, scene: &str) -> Result<MapMetadata, Error> {
        map::metadata(&self.root, scene)
    }

    /// Extract navigation-cooking metadata from exactly one named glTF node.
    ///
    /// A node without schema-1 navigation metadata returns `None`.
    pub fn navigation_metadata(&self, node: &str) -> Result<Option<NavigationMetadata>, Error> {
        navigation::metadata(&self.root, node)
    }

    /// Extract navigation metadata by stable glTF node index.
    ///
    /// This lets cookers validate navigation metadata on unnamed nodes rather
    /// than silently ignoring owned extras.
    pub fn navigation_metadata_at(
        &self,
        node_index: usize,
    ) -> Result<Option<NavigationMetadata>, Error> {
        navigation::metadata_at(&self.root, node_index)
    }

    /// Extract acoustic-cooking metadata from exactly one named glTF node.
    ///
    /// A node without schema-1 acoustics metadata returns `None`.
    pub fn acoustic_node_metadata(
        &self,
        node: &str,
    ) -> Result<Option<AcousticNodeMetadata>, Error> {
        acoustics::node_metadata(&self.root, node)
    }

    /// Extract acoustic metadata by stable glTF node index.
    pub fn acoustic_node_metadata_at(
        &self,
        node_index: usize,
    ) -> Result<Option<AcousticNodeMetadata>, Error> {
        acoustics::node_metadata_at(&self.root, node_index)
    }

    /// Extract the acoustic material asset referenced by one named glTF material.
    pub fn acoustic_material_metadata(
        &self,
        material: &str,
    ) -> Result<Option<AcousticMaterialMetadata>, Error> {
        acoustics::material_metadata(&self.root, material)
    }
}

fn parse_root(bytes: &[u8]) -> Result<Value, Error> {
    let json = container::json_bytes(bytes)?;
    let root: Value = serde_json::from_slice(json).map_err(Error::InvalidJson)?;
    if !root.is_object() {
        return Err(Error::InvalidRoot);
    }
    Ok(root)
}
