#![doc = include_str!("../README.md")]

mod catalog;
mod error;
mod hash;
mod id;
mod package;
#[cfg(feature = "hot-reload")]
mod reload;
mod signature;
mod store;
#[cfg(feature = "hot-reload")]
mod watcher;

pub use bytes::Bytes;
pub use catalog::{
    ASSET_CATALOG_SCHEMA, AssetAudience, AssetCatalog, AssetKind, AssetRecord,
    CookingProfileIdentity, ToolchainIdentity,
};
pub use error::{Error, InvalidAssetId, InvalidHash, InvalidPackageName, InvalidProfileName};
pub use hash::{
    AssetKeyId, AssetSetHash, ContentHash, PackageHash, PackagePayloadHash, ProfileHash, RecipeHash,
};
pub use id::{AssetId, PackageName, ProfileName};
pub use package::{AssetPackage, AssetReader, AuthenticatedAsset};
#[cfg(feature = "hot-reload")]
pub use reload::{
    AssetChange, AssetChangeKind, AssetChangeSet, AssetGeneration, AssetReload, AssetReloadStatus,
    AssetStoreManager, AssetStoreSnapshot,
};
pub use signature::AssetTrustStore;
#[cfg(feature = "signing")]
pub use signature::{AssetSigningKey, SigningKeyError, sign_package};
pub use store::{AssetStore, ResolvedAsset};
#[cfg(feature = "hot-reload")]
pub use watcher::{AssetStoreWatcher, AssetWatchEvent, AssetWatcherError};
