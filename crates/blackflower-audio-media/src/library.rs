use std::collections::BTreeMap;

use blackflower_assets::{AssetId, AssetKind};
use bytes::Bytes;

use crate::{AudioClip, AudioStream, Error, SoundEvent};

/// Parsed runtime audio object.
#[derive(Debug, Clone)]
pub enum AudioAsset {
    /// Memory-resident PCM clip.
    Clip(AudioClip),
    /// Lossless FLAC streaming media.
    Stream(AudioStream),
    /// Playback event policy.
    Event(SoundEvent),
}

/// Explicit runtime registry of parsed audio assets.
#[derive(Debug, Default)]
pub struct AudioLibrary {
    assets: BTreeMap<AssetId, AudioAsset>,
}

impl AudioLibrary {
    /// Create an empty library.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            assets: BTreeMap::new(),
        }
    }

    /// Parse and insert one catalog object.
    pub fn insert(&mut self, id: AssetId, kind: AssetKind, bytes: Bytes) -> Result<(), Error> {
        let asset = match kind {
            AssetKind::AudioClip => AudioAsset::Clip(AudioClip::from_bytes(bytes)?),
            AssetKind::AudioStream => AudioAsset::Stream(AudioStream::from_bytes(bytes)?),
            AssetKind::SoundEvent => AudioAsset::Event(SoundEvent::from_bytes(bytes)?),
            AssetKind::Map
            | AssetKind::Blob
            | AssetKind::LuauBytecode
            | AssetKind::ShaderModule
            | AssetKind::Texture2d
            | AssetKind::Mesh
            | AssetKind::Model
            | AssetKind::Volume
            | AssetKind::Skeleton
            | AssetKind::AnimationClip
            | AssetKind::NavigationMesh
            | AssetKind::AcousticMaterialLibrary
            | AssetKind::AcousticTopology
            | AssetKind::AcousticPrefab
            | AssetKind::AcousticSimulationScene
            | AssetKind::AcousticEmissionProfile
            | _ => return Err(Error::UnsupportedSource("asset kind is not audio")),
        };
        let _previous = self.assets.insert(id, asset);
        Ok(())
    }

    /// Find a parsed audio object.
    #[must_use]
    pub fn get(&self, id: &AssetId) -> Option<&AudioAsset> {
        self.assets.get(id)
    }

    /// Find a sound event.
    #[must_use]
    pub fn event(&self, id: &AssetId) -> Option<&SoundEvent> {
        match self.get(id) {
            Some(AudioAsset::Event(event)) => Some(event),
            _ => None,
        }
    }
}
