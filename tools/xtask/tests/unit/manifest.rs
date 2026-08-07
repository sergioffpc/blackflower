use std::fs;
use std::str::FromStr;

use anyhow::Context;
use blackflower_assets::{AssetAudience, AssetId, AssetKind, PackageName};
use tempfile::TempDir;

use super::{AssetSource, Repository};

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the fixture proves the complete map.toml to typed glTF boundary"
)]
fn map_manifest_selects_and_validates_typed_gltf_scene() -> anyhow::Result<()> {
    let directory = TempDir::new()?;
    let map = directory.path().join("maps/arena");
    fs::create_dir_all(&map)?;
    fs::write(
        map.join("arena.gltf"),
        br#"{
            "asset": {"version": "2.0"},
            "scenes": [
                {"name": "Arena", "nodes": [0]},
                {"name": "Player", "nodes": []}
            ],
            "nodes": [{
                "name": "North Spawn",
                "extras": {"blackflower": {
                    "schema": 1,
                    "node": {"id": "base_north", "role": "spawn_point"},
                    "spawn_point": {"set": "players", "weight": 1.0}
                }}
            }]
        }"#,
    )?;
    fs::write(
        map.join("map.toml"),
        r#"
schema = 1
id = "maps/arena"
source = "arena.gltf"
scene = "Arena"
player_model = "maps/arena/player"
"#,
    )?;
    write_player_model(&map)?;

    let repository = Repository::load(directory.path())?;
    let id = AssetId::from_str("maps/arena")?;
    let loaded = repository.maps.get(&id).context("map was not loaded")?;
    assert_eq!(loaded.id, id);
    assert_eq!(loaded.source_relative, "arena.gltf");
    assert_eq!(loaded.scene, "Arena");
    assert_eq!(loaded.metadata.nodes()[0].identifier(), "base_north");
    Ok(())
}

#[test]
fn map_manifest_id_is_derived_from_its_maps_path() -> anyhow::Result<()> {
    let directory = TempDir::new()?;
    let map = directory.path().join("maps/arena");
    fs::create_dir_all(&map)?;
    fs::write(
        map.join("arena.gltf"),
        br#"{
            "asset": {"version": "2.0"},
            "scenes": [
                {"name": "Arena", "nodes": []},
                {"name": "Player", "nodes": []}
            ],
            "nodes": []
        }"#,
    )?;
    fs::write(
        map.join("map.toml"),
        r#"
schema = 1
id = "maps/wrong"
source = "arena.gltf"
scene = "Arena"
player_model = "maps/arena/player"
"#,
    )?;
    write_player_model(&map)?;

    let Err(error) = Repository::load(directory.path()) else {
        anyhow::bail!("wrong map ID was accepted");
    };
    assert!(error.to_string().contains("must use ID `maps/arena`"));
    Ok(())
}

fn write_player_model(map: &std::path::Path) -> anyhow::Result<()> {
    fs::write(
        map.join("player.asset.toml"),
        r#"
schema = 1
id = "maps/arena/player"
kind = "model"
audience = "presentation"

[model]
source = "arena.gltf"
scene = "Player"
"#,
    )?;
    Ok(())
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the manifest fixture spells out every required navigation field"
)]
fn navigation_manifest_is_explicit_and_canonicalizes_areas() -> anyhow::Result<()> {
    let directory = TempDir::new()?;
    let asset = directory.path().join("level");
    fs::create_dir(&asset)?;
    fs::write(
        asset.join("navigation.gltf"),
        br#"{"asset":{"version":"2.0"}}"#,
    )?;
    fs::write(
        asset.join("asset.toml"),
        r#"
schema = 1
id = "levels/test/navigation/humanoid"
kind = "navigation_mesh"
audience = "simulation"

[navigation]
source = "navigation.gltf"
profile_id = "humanoid"

[navigation.agent]
height = 1.8
radius = 0.35
max_climb = 0.4
max_slope_degrees = 45.0

[navigation.build]
cell_size = 0.2
cell_height = 0.1
tile_size = 64
region_min_area = 8
region_merge_area = 20
max_edge_length = 12.0
max_simplification_error = 1.3
max_vertices_per_polygon = 6
detail_sample_distance = 6.0
detail_sample_max_error = 1.0

[[navigation.areas]]
key = "water"
traversable = false

[[navigation.areas]]
key = "ground"
traversable = true
cost = 1.0
"#,
    )?;

    let repository = Repository::load(directory.path())?;
    let loaded = repository
        .assets
        .values()
        .next()
        .context("navigation asset was not loaded")?;
    assert_eq!(loaded.manifest.kind(), AssetKind::NavigationMesh);
    assert_eq!(loaded.manifest.audience, AssetAudience::Simulation);
    let AssetSource::Navigation(navigation) = &loaded.manifest.source else {
        anyhow::bail!("loaded asset is not navigation");
    };
    assert_eq!(navigation.areas[0].key, "ground");
    assert_eq!(navigation.areas[1].key, "water");
    Ok(())
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the fixture spells out both sides of the source-less audio dependency"
)]
fn sound_event_is_source_less_and_closes_over_media() -> anyhow::Result<()> {
    let directory = TempDir::new()?;
    let media = directory.path().join("media");
    let event = directory.path().join("event");
    let package = directory.path().join("packages/pak000");
    fs::create_dir(&media)?;
    fs::create_dir(&event)?;
    fs::create_dir_all(&package)?;
    fs::write(media.join("shot.wav"), b"fixture")?;
    fs::write(
        media.join("asset.toml"),
        r#"
schema = 1
id = "audio/shot"
kind = "audio_clip"
audience = "presentation"

[audio_clip]
source = "shot.wav"
"#,
    )?;
    fs::write(
        event.join("asset.toml"),
        r#"
schema = 1
id = "sound_events/shot"
kind = "sound_event"
audience = "presentation"

[sound_event]
media = "audio/shot"
gain_db = 0.0
priority = 100
spatialization = "hrtf"
"#,
    )?;
    fs::write(
        package.join("package.toml"),
        r#"
schema = 1
assets = ["sound_events/shot"]
"#,
    )?;

    let repository = Repository::load(directory.path())?;
    let selected = repository.selected_assets(&PackageName::from_str("pak000")?)?;
    assert_eq!(selected.len(), 2);
    assert!(selected.contains(&AssetId::from_str("audio/shot")?));
    let event_id = AssetId::from_str("sound_events/shot")?;
    let loaded = repository
        .assets
        .get(&event_id)
        .context("sound event was not loaded")?;
    assert!(loaded.source_bytes.is_empty());
    assert_eq!(loaded.manifest.kind(), AssetKind::SoundEvent);
    Ok(())
}
