use std::str::FromStr;

use crate::{
    AssetAudience, AssetId, AssetKind, AssetStore, AuthenticatedAsset, Error, InvalidAssetId,
};

const MAGIC: &[u8; 8] = b"BFMAP\0\0\0";

/// Current signed map-descriptor schema.
pub const MAP_ASSET_SCHEMA: u32 = 1;

/// Authenticated runtime selection of the local player's presentation model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapAsset {
    player_model: AssetId,
}

impl MapAsset {
    /// Create a canonical map descriptor.
    #[must_use]
    pub const fn new(player_model: AssetId) -> Self {
        Self { player_model }
    }

    /// Return the model used for the local player in this map.
    #[must_use]
    pub const fn player_model(&self) -> &AssetId {
        &self.player_model
    }

    /// Encode the canonical runtime descriptor.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let model = self.player_model.as_str().as_bytes();
        let mut bytes = Vec::with_capacity(MAGIC.len() + 6 + model.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&MAP_ASSET_SCHEMA.to_le_bytes());
        bytes.extend_from_slice(&u16::try_from(model.len()).unwrap_or(u16::MAX).to_le_bytes());
        bytes.extend_from_slice(model);
        bytes
    }

    /// Decode a canonical descriptor without asserting package provenance.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid magic, schema, length, UTF-8, asset IDs, or
    /// trailing bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MapAssetError> {
        let header_len = MAGIC.len() + size_of::<u32>() + size_of::<u16>();
        if bytes.len() < header_len || &bytes[..MAGIC.len()] != MAGIC {
            return Err(MapAssetError::InvalidFormat("invalid map asset magic"));
        }
        let schema_offset = MAGIC.len();
        let schema = u32::from_le_bytes(
            bytes[schema_offset..schema_offset + size_of::<u32>()]
                .try_into()
                .map_err(|_error| MapAssetError::InvalidFormat("truncated map asset schema"))?,
        );
        if schema != MAP_ASSET_SCHEMA {
            return Err(MapAssetError::UnsupportedSchema(schema));
        }
        let length_offset = schema_offset + size_of::<u32>();
        let length = usize::from(u16::from_le_bytes(
            bytes[length_offset..header_len]
                .try_into()
                .map_err(|_error| MapAssetError::InvalidFormat("truncated map asset length"))?,
        ));
        let end = header_len
            .checked_add(length)
            .ok_or(MapAssetError::InvalidFormat("map asset length overflow"))?;
        if end != bytes.len() {
            return Err(MapAssetError::InvalidFormat(
                "map asset model length does not match payload",
            ));
        }
        let model = std::str::from_utf8(&bytes[header_len..end])
            .map_err(|_error| MapAssetError::InvalidUtf8)?;
        Ok(Self::new(AssetId::from_str(model)?))
    }

    /// Decode a descriptor whose package signature, record, and bytes were verified.
    ///
    /// # Errors
    ///
    /// Returns an error when the authenticated record is not a shared Map, its
    /// dependency does not match the encoded model, or the bytes are invalid.
    pub fn from_authenticated(asset: AuthenticatedAsset) -> Result<Self, MapAssetError> {
        let record = asset.record();
        if record.kind != AssetKind::Map {
            return Err(MapAssetError::InvalidKind(record.kind));
        }
        if record.audience != AssetAudience::Shared {
            return Err(MapAssetError::InvalidAudience(record.audience));
        }
        let map = Self::from_bytes(asset.bytes())?;
        if record.dependencies.as_slice() != [map.player_model.clone()] {
            return Err(MapAssetError::DependencyMismatch);
        }
        Ok(map)
    }

    /// Load the winning signed map descriptor and validate its model dependency.
    ///
    /// # Errors
    ///
    /// Returns an error when the map or selected model is absent, corrupt,
    /// incorrectly typed, or assigned to the wrong runtime audience.
    pub fn load(store: &AssetStore, id: &AssetId) -> Result<Self, MapAssetError> {
        let map = Self::from_authenticated(store.read_authenticated_asset(id)?)?;
        let model = store
            .resolve(map.player_model())
            .ok_or_else(|| MapAssetError::MissingPlayerModel(map.player_model.clone()))?;
        if model.record().kind != AssetKind::Model {
            return Err(MapAssetError::InvalidPlayerModelKind(model.record().kind));
        }
        if model.record().audience != AssetAudience::Presentation {
            return Err(MapAssetError::InvalidPlayerModelAudience(
                model.record().audience,
            ));
        }
        Ok(map)
    }
}

/// Failure while decoding or validating a signed map descriptor.
#[derive(Debug, thiserror::Error)]
pub enum MapAssetError {
    /// The map or one of its dependencies could not be read from the store.
    #[error(transparent)]
    Store(#[from] Error),
    /// The encoded player-model ID is not canonical.
    #[error(transparent)]
    InvalidAssetId(#[from] InvalidAssetId),
    /// The descriptor bytes are structurally invalid.
    #[error("invalid map asset: {0}")]
    InvalidFormat(&'static str),
    /// The player-model text is not UTF-8.
    #[error("map asset player model is not UTF-8")]
    InvalidUtf8,
    /// The descriptor schema is not supported.
    #[error("unsupported map asset schema {0}")]
    UnsupportedSchema(u32),
    /// The signed catalog record does not identify a map.
    #[error("signed map asset has kind {0:?}")]
    InvalidKind(AssetKind),
    /// Map selection must be shared by simulation and presentation.
    #[error("signed map asset has audience {0:?}, expected shared")]
    InvalidAudience(AssetAudience),
    /// The signed dependency edge and encoded player model differ.
    #[error("signed map asset dependency does not match its encoded player model")]
    DependencyMismatch,
    /// The selected player model is absent from the winning package overlay.
    #[error("signed map asset references missing player model `{0}`")]
    MissingPlayerModel(AssetId),
    /// The selected presentation resource is not a model.
    #[error("signed map asset player resource has kind {0:?}, expected model")]
    InvalidPlayerModelKind(AssetKind),
    /// The selected model is not available to presentation.
    #[error("signed map asset player model has audience {0:?}, expected presentation")]
    InvalidPlayerModelAudience(AssetAudience),
}

#[cfg(test)]
#[path = "../tests/unit/map.rs"]
mod tests;
