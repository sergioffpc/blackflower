use anyhow::Context;
use blackflower_assets::Bytes;
use blackflower_navigation::{
    NavAgentProfile, NavAgentProfileId, NavigationArea, NavigationAreaKey, NavigationBuildSettings,
};

use crate::manifest::{LoadedAsset, NavigationManifest};

pub(crate) struct CookedNavigation {
    pub(crate) bytes: Bytes,
    pub(crate) source_hash: blake3::Hash,
}

pub(crate) fn cook(
    source: &LoadedAsset,
    manifest: &NavigationManifest,
) -> anyhow::Result<CookedNavigation> {
    let profile_id = NavAgentProfileId::new(manifest.profile_id.clone())
        .context("invalid navigation profile ID")?;
    let agent = NavAgentProfile::new(
        profile_id,
        manifest.agent.height,
        manifest.agent.radius,
        manifest.agent.max_climb,
        manifest.agent.max_slope_degrees,
    )
    .context("invalid navigation agent profile")?;
    let build = NavigationBuildSettings::new(
        manifest.build.cell_size,
        manifest.build.cell_height,
        manifest.build.tile_size,
        manifest.build.region_min_area,
        manifest.build.region_merge_area,
        manifest.build.max_edge_length,
        manifest.build.max_simplification_error,
        manifest.build.max_vertices_per_polygon,
        manifest.build.detail_sample_distance,
        manifest.build.detail_sample_max_error,
    )
    .context("invalid navigation build settings")?;
    let areas = manifest
        .areas
        .iter()
        .enumerate()
        .map(|(id, area)| {
            let id = u8::try_from(id).context("navigation area index exceeds u8")?;
            let key =
                NavigationAreaKey::new(area.key.clone()).context("invalid navigation area key")?;
            NavigationArea::new(id, key, area.traversable, area.cost)
                .context("invalid navigation area")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let cooked = blackflower_cooker_navigation::cook(&source.source_path, agent, build, areas)
        .context("navigation cooker rejected glTF source")?;
    Ok(CookedNavigation {
        bytes: cooked.asset.bytes().clone(),
        source_hash: cooked.source_hash,
    })
}

pub(crate) fn platform_identity() -> String {
    format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH)
}
