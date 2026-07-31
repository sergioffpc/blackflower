#![doc = include_str!("../README.md")]

mod error;
mod ffi;
mod geometry;

use std::path::Path;

use blackflower_navigation::{
    NavAgentProfile, NavMeshAsset, NavigationArea, NavigationBuildSettings,
};

pub use error::Error;

/// Pinned RecastNavigation release used by the offline cooker.
pub const RECAST_VERSION: &str = "1.6.0";
/// Exact pinned RecastNavigation source revision.
pub const RECAST_REVISION: &str = "6dc1667f580357e8a2154c28b7867bea7e8ad3a7";
/// Versioned GLB-to-Recast cooking recipe.
pub const COOKER_RECIPE: &str =
    "blackflower-cooker-navigation-v1;tiled;watershed;polyref=32;flags=traversable";

/// Cooked runtime asset plus imported external-buffer identity.
pub struct CookedNavigation {
    /// Canonical `.bfnav` runtime asset.
    pub asset: NavMeshAsset,
    /// Hash of every glTF buffer in declaration order.
    pub source_hash: blake3::Hash,
}

/// Cook marked glTF geometry into deterministic tiled Detour data.
///
/// All agent dimensions, build settings, and semantic area policy are
/// explicit inputs originating in the asset manifest. Lua is not consulted.
pub fn cook(
    source: &Path,
    agent: NavAgentProfile,
    build: NavigationBuildSettings,
    areas: Vec<NavigationArea>,
) -> Result<CookedNavigation, Error> {
    let (geometry, source_hash) = geometry::import(source, &areas)?;
    let native_areas = geometry::native_areas(&geometry, &areas)?;
    let cooked = ffi::cook(&geometry, &native_areas, &agent, &build)?;
    let asset = NavMeshAsset::new(agent, build, cooked.params, areas, cooked.tiles)
        .map_err(Error::InvalidOutput)?;
    let decoded = NavMeshAsset::from_bytes(asset.bytes().clone()).map_err(Error::InvalidOutput)?;
    let _runtime = decoded.instantiate().map_err(Error::InvalidOutput)?;
    Ok(CookedNavigation { asset, source_hash })
}
