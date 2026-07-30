#![doc = include_str!("../README.md")]

mod catalog;
mod error;
mod hash;
mod id;
mod package;
mod signature;
mod store;

pub use bytes::Bytes;
pub use catalog::{
    ASSET_CATALOG_SCHEMA, AssetAudience, AssetCatalog, AssetKind, AssetRecord, ToolchainIdentity,
};
pub use error::{Error, InvalidAssetId, InvalidHash, InvalidPackageName};
pub use hash::{
    AssetKeyId, AssetSetHash, ContentHash, PackageHash, PackagePayloadHash, RecipeHash,
};
pub use id::{AssetId, PackageName};
pub use package::{AssetPackage, AssetReader};
pub use signature::AssetTrustStore;
#[cfg(feature = "signing")]
pub use signature::{AssetSigningKey, SigningKeyError, sign_package};
pub use store::{AssetStore, ResolvedAsset};
