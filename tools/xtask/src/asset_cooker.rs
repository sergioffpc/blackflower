use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use blackflower_assets::{AssetAudience, AssetId, AssetKind, Bytes, ContentHash, RecipeHash};
use blackflower_scripting::{compile, luau_version};

use crate::manifest::{AssetSource, LoadedAsset, Repository};
use crate::profile::CookingProfile;

#[derive(Debug)]
pub(crate) struct CookedAsset {
    pub(crate) kind: AssetKind,
    pub(crate) audience: AssetAudience,
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) bytes: Bytes,
    pub(crate) content_hash: ContentHash,
    pub(crate) recipe_hash: RecipeHash,
}

pub(crate) fn cook_assets(
    repository: &Repository,
    selected: &BTreeSet<AssetId>,
    profile: &CookingProfile,
) -> anyhow::Result<BTreeMap<AssetId, CookedAsset>> {
    let mut cooked = BTreeMap::new();
    for id in selected {
        let source = repository
            .assets
            .get(id)
            .with_context(|| format!("missing selected asset `{id}`"))?;
        let bytes =
            cook_asset(source, profile).with_context(|| format!("failed to cook asset `{id}`"))?;
        let content_hash = ContentHash::hash_bytes(&bytes);
        let recipe_hash = recipe_hash(source, profile)?;
        cooked.insert(
            id.clone(),
            CookedAsset {
                kind: source.manifest.kind(),
                audience: source.manifest.audience,
                dependencies: Vec::new(),
                bytes,
                content_hash,
                recipe_hash,
            },
        );
    }
    Ok(cooked)
}

fn cook_asset(source: &LoadedAsset, profile: &CookingProfile) -> anyhow::Result<Bytes> {
    match &source.manifest.source {
        AssetSource::Blob(_) => Ok(Bytes::from(source.source_bytes.clone())),
        AssetSource::Luau(_) => {
            let text =
                std::str::from_utf8(&source.source_bytes).context("Luau source is not UTF-8")?;
            let bytecode = compile(text, profile.scripting.luau.compile_options())
                .context("Luau compiler rejected source")?;
            Ok(Bytes::from(bytecode.into_bytes()))
        }
    }
}

fn recipe_hash(source: &LoadedAsset, profile: &CookingProfile) -> anyhow::Result<RecipeHash> {
    let mut hasher = CanonicalHasher::new(b"blackflower.asset-recipe.v2");
    hasher.u32(source.manifest.schema);
    hasher.text(source.manifest.id.as_str());
    hasher.serializable(&source.manifest.kind())?;
    hasher.serializable(&source.manifest.audience)?;
    hasher.text(&source.source_relative);
    hasher.bytes(source.source_hash.as_bytes());
    hasher.text(env!("CARGO_PKG_VERSION"));
    match &source.manifest.source {
        AssetSource::Blob(_) => hasher.text("blob"),
        AssetSource::Luau(_) => {
            hasher.text("luau");
            hasher.serializable(&profile.scripting.luau)?;
            let (major, minor, patch) = luau_version();
            hasher.u32(major);
            hasher.u32(minor);
            hasher.u32(patch);
        }
    }
    Ok(RecipeHash::from_bytes(*hasher.finish().as_bytes()))
}

struct CanonicalHasher(blake3::Hasher);

impl CanonicalHasher {
    fn new(domain: &[u8]) -> Self {
        let mut value = Self(blake3::Hasher::new());
        value.bytes(domain);
        value
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        self.0.update(bytes);
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.0.update(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.update(&value.to_le_bytes());
    }

    fn serializable(&mut self, value: &impl serde::Serialize) -> anyhow::Result<()> {
        self.bytes(&serde_json::to_vec(value)?);
        Ok(())
    }

    fn finish(self) -> blake3::Hash {
        self.0.finalize()
    }
}
