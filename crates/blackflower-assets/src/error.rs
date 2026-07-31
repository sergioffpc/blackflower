use std::path::PathBuf;

use crate::{
    AssetAudience, AssetId, AssetKeyId, AssetKind, AssetSetHash, CookingProfileIdentity,
    PackageName,
};

/// An asset identifier did not follow the canonical portable grammar.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid asset identifier `{value}`: {reason}")]
pub struct InvalidAssetId {
    pub(crate) value: String,
    pub(crate) reason: &'static str,
}

impl InvalidAssetId {
    pub(crate) fn new(value: impl Into<String>, reason: &'static str) -> Self {
        Self {
            value: value.into(),
            reason,
        }
    }
}

/// A package name did not follow the canonical portable grammar.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid package name `{value}`: {reason}")]
pub struct InvalidPackageName {
    pub(crate) value: String,
    pub(crate) reason: &'static str,
}

/// A cooking profile name did not follow the canonical portable grammar.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid cooking profile name `{value}`: {reason}")]
pub struct InvalidProfileName {
    pub(crate) value: String,
    pub(crate) reason: &'static str,
}

impl InvalidProfileName {
    pub(crate) fn new(value: impl Into<String>, reason: &'static str) -> Self {
        Self {
            value: value.into(),
            reason,
        }
    }
}

impl InvalidPackageName {
    pub(crate) fn new(value: impl Into<String>, reason: &'static str) -> Self {
        Self {
            value: value.into(),
            reason,
        }
    }
}

/// A BLAKE3 identity was not exactly 32 bytes encoded as lowercase hexadecimal.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid BLAKE3 hash `{0}`")]
pub struct InvalidHash(pub(crate) String);

/// Errors produced while opening or reading cooked asset packages.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A filesystem operation failed.
    #[error("asset filesystem operation failed for `{path}`")]
    Io {
        /// Path being accessed.
        path: PathBuf,
        /// Underlying operating-system error.
        #[source]
        source: std::io::Error,
    },
    /// A direct child with a SquashFS suffix was not a regular file.
    #[error("asset package `{0}` is not a regular file")]
    PackageNotFile(PathBuf),
    /// A package filename was invalid.
    #[error(transparent)]
    InvalidPackageName(#[from] InvalidPackageName),
    /// A cooking profile name was invalid.
    #[error(transparent)]
    InvalidProfileName(#[from] InvalidProfileName),
    /// An asset ID was invalid.
    #[error(transparent)]
    InvalidAssetId(#[from] InvalidAssetId),
    /// A serialized hash was invalid.
    #[error(transparent)]
    InvalidHash(#[from] InvalidHash),
    /// The package bytes did not match a required identity.
    #[error("package hash mismatch for `{path}`: expected {expected}, found {actual}")]
    PackageHashMismatch {
        /// Package that was verified.
        path: PathBuf,
        /// Required package identity.
        expected: crate::PackageHash,
        /// Identity calculated from the package bytes.
        actual: crate::PackageHash,
    },
    /// The file did not end in a supported Blackflower signature footer.
    #[error("invalid asset package signature footer in `{path}`: {reason}")]
    InvalidSignatureFooter {
        /// Package being verified.
        path: PathBuf,
        /// Rejected footer invariant.
        reason: &'static str,
    },
    /// The signing key was not supplied by the executable.
    #[error("asset package `{path}` was signed by untrusted key `{key_id}`")]
    UntrustedSigningKey {
        /// Package being verified.
        path: PathBuf,
        /// Key selected by the footer.
        key_id: AssetKeyId,
    },
    /// The signature did not authenticate the package's BLAKE3 payload digest.
    #[error("invalid signature for asset package `{path}` from key `{key_id}`")]
    InvalidPackageSignature {
        /// Package being verified.
        path: PathBuf,
        /// Trusted key that failed verification.
        key_id: AssetKeyId,
        /// Ed25519 verification failure.
        #[source]
        source: ed25519_dalek::SignatureError,
    },
    /// A public key supplied by the executable was malformed or weak.
    #[error("invalid trusted Ed25519 public key: {reason}")]
    InvalidPublicKey {
        /// Rejected key property.
        reason: String,
    },
    /// Backhand rejected the SquashFS image.
    #[error("invalid SquashFS asset package `{path}`")]
    Squashfs {
        /// Package path.
        path: PathBuf,
        /// Underlying SquashFS error.
        #[source]
        source: backhand::BackhandError,
    },
    /// The package does not use the fixed runtime archive configuration.
    #[error("unsupported asset package configuration in `{path}`: {reason}")]
    UnsupportedPackage {
        /// Package path.
        path: PathBuf,
        /// Rejected configuration detail.
        reason: &'static str,
    },
    /// The package did not contain the catalog at its fixed path.
    #[error("asset package `{0}` is missing `/blackflower/catalog.toml`")]
    MissingCatalog(PathBuf),
    /// The embedded catalog was not valid TOML.
    #[error("invalid asset catalog in `{path}`")]
    CatalogToml {
        /// Package path.
        path: PathBuf,
        /// TOML parsing error.
        #[source]
        source: toml::de::Error,
    },
    /// The catalog uses an unsupported schema.
    #[error("unsupported asset catalog schema {schema} in `{path}`")]
    UnsupportedCatalogSchema {
        /// Package path.
        path: PathBuf,
        /// Schema found in the package.
        schema: u32,
    },
    /// The catalog violated a deterministic ordering or uniqueness invariant.
    #[error("invalid asset catalog in `{path}`: {reason}")]
    InvalidCatalog {
        /// Package path.
        path: PathBuf,
        /// Violated invariant.
        reason: String,
    },
    /// An object listed by the catalog was absent from the archive.
    #[error("asset `{asset}` references missing object `{object_path}` in `{package}`")]
    MissingObject {
        /// Asset whose object was missing.
        asset: AssetId,
        /// Expected archive path.
        object_path: String,
        /// Package path.
        package: PathBuf,
    },
    /// An object had a size different from the catalog.
    #[error("asset `{asset}` has size {actual} but catalog declares {expected} in `{package}`")]
    ObjectSizeMismatch {
        /// Asset whose object had the wrong size.
        asset: AssetId,
        /// Declared byte length.
        expected: u64,
        /// SquashFS inode byte length.
        actual: u64,
        /// Package path.
        package: PathBuf,
    },
    /// An asset ID was not resolved by any package.
    #[error("asset `{0}` was not found")]
    AssetNotFound(AssetId),
    /// An override attempted to change the stable type or audience of an asset ID.
    #[error(
        "incompatible override for `{asset}` in `{package}`: expected {expected_kind:?}/{expected_audience:?}, found {actual_kind:?}/{actual_audience:?}"
    )]
    IncompatibleOverride {
        /// Overridden asset ID.
        asset: AssetId,
        /// Package containing the incompatible record.
        package: PackageName,
        /// Kind established by a lower-priority package.
        expected_kind: AssetKind,
        /// Audience established by a lower-priority package.
        expected_audience: AssetAudience,
        /// Kind supplied by the override.
        actual_kind: AssetKind,
        /// Audience supplied by the override.
        actual_audience: AssetAudience,
    },
    /// A winning record has a dependency absent from the layered store.
    #[error("asset `{asset}` depends on missing asset `{dependency}`")]
    MissingDependency {
        /// Asset declaring the dependency.
        asset: AssetId,
        /// Unresolved dependency.
        dependency: AssetId,
    },
    /// A winning dependency has a runtime kind incompatible with its consumer.
    #[error(
        "asset `{asset}` dependency `{dependency}` must be {expected:?}, but resolved as {actual:?}"
    )]
    DependencyKindMismatch {
        /// Asset declaring the dependency.
        asset: AssetId,
        /// Dependency with the wrong kind.
        dependency: AssetId,
        /// Required runtime kind.
        expected: AssetKind,
        /// Resolved runtime kind.
        actual: AssetKind,
    },
    /// A model attachment resolved to a runtime kind outside the model contract.
    #[error(
        "model `{asset}` attachment `{dependency}` must resolve as Mesh or Volume, but resolved as {actual:?}"
    )]
    InvalidModelAttachmentKind {
        /// Model declaring the attachment dependency.
        asset: AssetId,
        /// Attachment with the wrong kind.
        dependency: AssetId,
        /// Resolved runtime kind.
        actual: AssetKind,
    },
    /// Packages in one layered store were cooked with different profiles.
    #[error(
        "asset package `{package}` uses cooking profile {actual:?}, but the store expects {expected:?}"
    )]
    IncompatibleProfile {
        /// Package containing the incompatible profile identity.
        package: PackageName,
        /// Profile identity established by the first package.
        expected: Box<CookingProfileIdentity>,
        /// Profile identity supplied by the incompatible package.
        actual: Box<CookingProfileIdentity>,
    },
    /// Hot reload attempted to change the stable contract of an existing ID.
    #[cfg(feature = "hot-reload")]
    #[error(
        "hot reload reclassified `{asset}`: expected {expected_kind:?}/{expected_audience:?}, found {actual_kind:?}/{actual_audience:?}"
    )]
    HotReloadReclassification {
        /// Asset whose runtime contract changed.
        asset: AssetId,
        /// Kind in the currently published snapshot.
        expected_kind: AssetKind,
        /// Audience in the currently published snapshot.
        expected_audience: AssetAudience,
        /// Kind in the candidate snapshot.
        actual_kind: AssetKind,
        /// Audience in the candidate snapshot.
        actual_audience: AssetAudience,
    },
    /// No further successful store generations can be represented.
    #[cfg(feature = "hot-reload")]
    #[error("asset store generation counter is exhausted")]
    AssetGenerationExhausted,
    /// The exact ordered package set did not match the required identity.
    #[error("asset set hash mismatch: expected {expected}, found {actual}")]
    AssetSetHashMismatch {
        /// Required identity.
        expected: AssetSetHash,
        /// Opened identity.
        actual: AssetSetHash,
    },
}

pub(crate) fn io_error(path: impl Into<PathBuf>, source: std::io::Error) -> Error {
    Error::Io {
        path: path.into(),
        source,
    }
}
