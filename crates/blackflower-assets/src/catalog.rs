use serde::{Deserialize, Serialize};

use crate::{AssetId, ContentHash, ProfileHash, ProfileName, RecipeHash};

/// Current embedded JSON catalog schema.
pub const ASSET_CATALOG_SCHEMA: u32 = 1;

/// Runtime representation used to decode an asset object.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    /// Opaque bytes used by the pipeline foundation and fixtures.
    Blob,
    /// Bytecode produced for the pinned Luau virtual machine.
    LuauBytecode,
    /// Naga-validated SPIR-V produced by the pinned Slang compiler.
    ShaderModule,
    /// KTX2 two-dimensional texture with Blackflower semantic metadata.
    Texture2d,
    /// Optimized static mesh primitives with a generated LOD chain.
    Mesh,
}

/// Runtime domains that consume an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetAudience {
    /// Authoritative simulation and prediction.
    Simulation,
    /// Rendering, mixing, and other presentation systems.
    Presentation,
    /// The exact cooked artifact is consumed by both domains.
    Shared,
}

/// Reproducible cooker and package-format identity recorded in every catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainIdentity {
    /// Blackflower cooker crate version.
    pub cooker: String,
    /// SquashFS implementation and version.
    pub squashfs: String,
    /// Fixed archive format settings.
    pub archive: String,
    /// Luau compiler and bytecode version.
    pub luau: String,
    /// Slang compiler version used to emit shader modules.
    pub slang: String,
    /// Naga version used to validate shader modules.
    pub naga: String,
    /// KTX-Software version used to encode and transcode textures.
    pub ktx: String,
    /// Image decoder and half-float conversion versions.
    pub texture_decoder: String,
    /// Host platform participating in native BasisU output identity.
    pub texture_encoder_platform: String,
    /// Rust meshoptimizer wrapper version used for optimization and LODs.
    pub meshoptimizer: String,
}

/// Exact versioned cooking profile used to produce a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CookingProfileIdentity {
    /// Portable profile filename stem.
    pub name: ProfileName,
    /// Hash of the profile's canonical semantic configuration.
    pub hash: ProfileHash,
}

/// Catalog entry mapping a logical ID to one content-addressed archive object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetRecord {
    /// Logical asset ID.
    pub id: AssetId,
    /// Runtime representation.
    pub kind: AssetKind,
    /// Runtime audience.
    pub audience: AssetAudience,
    /// Cooker-derived runtime requirements.
    pub dependencies: Vec<AssetId>,
    /// Hash of final cooked bytes.
    pub content_hash: ContentHash,
    /// Hash of the complete cooker recipe.
    pub recipe_hash: RecipeHash,
    /// Number of uncompressed object bytes.
    pub byte_len: u64,
    /// Fixed path to the object inside SquashFS.
    pub object_path: String,
}

/// Strict catalog embedded at `/blackflower/catalog.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetCatalog {
    /// Catalog schema.
    pub schema: u32,
    /// Exact cooking profile.
    pub profile: CookingProfileIdentity,
    /// Exact toolchain configuration that produced the package.
    pub toolchain: ToolchainIdentity,
    /// Records ordered by logical asset ID.
    pub assets: Vec<AssetRecord>,
}

impl AssetCatalog {
    /// Finds a record using the catalog's canonical ID ordering.
    #[must_use]
    pub fn find(&self, id: &AssetId) -> Option<&AssetRecord> {
        self.assets
            .binary_search_by(|record| record.id.cmp(id))
            .ok()
            .map(|index| &self.assets[index])
    }
}
