use std::fs;

use blackflower_cooker_navigation::cook;
use blackflower_navigation::{
    NavAgentProfile, NavAgentProfileId, NavigationArea, NavigationAreaKey, NavigationBuildSettings,
    PathPointKind,
};
use glam::Vec3A;
use tempfile::TempDir;

#[test]
fn marked_gltf_surface_cooks_deterministically_across_tiles()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = TempDir::new()?;
    let source = write_rectangle_source(&directory, "ground", 30.0, 30.0)?;
    let first = cook_asset(&source, vec![area(0, "ground", true, Some(1.0))?])?;
    let second = cook_asset(&source, vec![area(0, "ground", true, Some(1.0))?])?;
    assert_eq!(first.asset.bytes(), second.asset.bytes());
    assert!(first.asset.tiles().len() > 1);

    let navmesh = first.asset.instantiate()?;
    let path = navmesh.query()?.find_path(
        Vec3A::new(1.0, 0.0, 1.0),
        Vec3A::new(29.0, 0.0, 29.0),
        Vec3A::new(2.0, 4.0, 2.0),
        &first.asset.query_filter()?,
    )?;
    assert!(!path.is_partial());
    Ok(())
}

#[test]
fn blocked_area_produces_no_queryable_polygon() -> Result<(), Box<dyn std::error::Error>> {
    let directory = TempDir::new()?;
    let source = write_rectangle_source(&directory, "water", 10.0, 10.0)?;
    let cooked = cook_asset(
        &source,
        vec![
            area(0, "ground", true, Some(1.0))?,
            area(1, "water", false, None)?,
        ],
    )?;
    let navmesh = cooked.asset.instantiate()?;
    assert!(
        navmesh
            .query()?
            .nearest_point(
                Vec3A::new(5.0, 0.0, 5.0),
                Vec3A::new(2.0, 4.0, 2.0),
                &cooked.asset.query_filter()?,
            )?
            .is_none()
    );
    Ok(())
}

#[test]
fn authored_off_mesh_link_connects_disconnected_surfaces() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = TempDir::new()?;
    let source = write_link_source(&directory)?;
    let cooked = cook_asset(
        &source,
        vec![
            area(0, "ground", true, Some(1.0))?,
            area(1, "jump", true, Some(1.0))?,
        ],
    )?;
    let navmesh = cooked.asset.instantiate()?;
    let path = navmesh.query()?.find_path(
        Vec3A::new(1.0, 0.0, 2.0),
        Vec3A::new(9.0, 0.0, 2.0),
        Vec3A::new(2.0, 4.0, 2.0),
        &cooked.asset.query_filter()?,
    )?;
    assert!(!path.is_partial());
    assert!(
        path.points()
            .iter()
            .any(|point| point.kind() == PathPointKind::OffMeshConnection)
    );
    Ok(())
}

#[test]
fn authored_area_cost_changes_the_selected_route() -> Result<(), Box<dyn std::error::Error>> {
    let directory = TempDir::new()?;
    let source = write_cost_source(&directory)?;
    let cheap = cook_asset(
        &source,
        vec![
            area(0, "ground", true, Some(1.0))?,
            area(1, "mud", true, Some(1.0))?,
        ],
    )?;
    let expensive = cook_asset(
        &source,
        vec![
            area(0, "ground", true, Some(1.0))?,
            area(1, "mud", true, Some(100.0))?,
        ],
    )?;
    let start = Vec3A::new(1.0, 0.0, 5.0);
    let end = Vec3A::new(19.0, 0.0, 5.0);
    let extents = Vec3A::new(2.0, 4.0, 2.0);
    let cheap_mesh = cheap.asset.instantiate()?;
    let cheap_path =
        cheap_mesh
            .query()?
            .find_path(start, end, extents, &cheap.asset.query_filter()?)?;
    let expensive_mesh = expensive.asset.instantiate()?;
    let expensive_path =
        expensive_mesh
            .query()?
            .find_path(start, end, extents, &expensive.asset.query_filter()?)?;

    assert_eq!(cheap_path.points().len(), 2);
    assert!(expensive_path.points().len() > cheap_path.points().len());
    assert!(
        expensive_path
            .points()
            .iter()
            .any(|point| point.position().z < 2.5 || point.position().z > 7.5)
    );
    Ok(())
}

#[test]
fn physical_agent_radius_changes_narrow_surface_viability() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = TempDir::new()?;
    let source = write_rectangle_source(&directory, "ground", 10.0, 2.0)?;
    let small = cook_asset_with_radius(&source, 0.2, vec![area(0, "ground", true, Some(1.0))?])?;
    assert!(!small.asset.tiles().is_empty());
    assert!(
        cook_asset_with_radius(&source, 1.2, vec![area(0, "ground", true, Some(1.0))?],).is_err()
    );
    Ok(())
}

fn cook_asset(
    source: &std::path::Path,
    areas: Vec<NavigationArea>,
) -> Result<blackflower_cooker_navigation::CookedNavigation, Box<dyn std::error::Error>> {
    cook_asset_with_radius(source, 0.35, areas)
}

fn cook_asset_with_radius(
    source: &std::path::Path,
    radius: f32,
    areas: Vec<NavigationArea>,
) -> Result<blackflower_cooker_navigation::CookedNavigation, Box<dyn std::error::Error>> {
    Ok(cook(
        source,
        NavAgentProfile::new(NavAgentProfileId::new("humanoid")?, 1.8, radius, 0.4, 45.0)?,
        NavigationBuildSettings::new(0.2, 0.1, 64, 1, 1, 12.0, 1.3, 6, 6.0, 1.0)?,
        areas,
    )?)
}

fn area(
    id: u8,
    key: &str,
    traversable: bool,
    cost: Option<f32>,
) -> Result<NavigationArea, blackflower_navigation::Error> {
    NavigationArea::new(id, NavigationAreaKey::new(key)?, traversable, cost)
}

#[allow(
    clippy::too_many_lines,
    reason = "the self-contained glTF fixture spells out the complete binary accessor layout"
)]
fn write_rectangle_source(
    directory: &TempDir,
    area_key: &str,
    extent_x: f32,
    extent_z: f32,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let mut buffer = Vec::new();
    for value in [
        0.0_f32, 0.0, 0.0, 0.0, 0.0, extent_z, extent_x, 0.0, extent_z, extent_x, 0.0, 0.0,
    ] {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    for index in [0_u32, 1, 2, 0, 2, 3] {
        buffer.extend_from_slice(&index.to_le_bytes());
    }
    fs::write(directory.path().join("floor.bin"), &buffer)?;
    let source = directory.path().join("floor.gltf");
    let json = format!(
        r#"{{
            "asset": {{"version": "2.0"}},
            "scene": 0,
            "scenes": [{{"nodes": [0]}}],
            "nodes": [{{
                "name": "Navigation Floor",
                "mesh": 0,
                "extras": {{"blackflower": {{
                    "schema": 1,
                    "node": {{"kind": "navigation_surface", "id": "floor_main"}},
                    "navigation": {{"role": "surface", "area_key": "{area_key}"}}
                }}}}
            }}],
            "meshes": [{{"primitives": [{{
                "attributes": {{"POSITION": 0}},
                "indices": 1,
                "mode": 4
            }}]}}],
            "buffers": [{{"uri": "floor.bin", "byteLength": 72}}],
            "bufferViews": [
                {{"buffer": 0, "byteOffset": 0, "byteLength": 48}},
                {{"buffer": 0, "byteOffset": 48, "byteLength": 24}}
            ],
            "accessors": [
                {{
                    "bufferView": 0,
                    "componentType": 5126,
                    "count": 4,
                    "type": "VEC3",
                    "min": [0, 0, 0],
                    "max": [{extent_x}, 0, {extent_z}]
                }},
                {{
                    "bufferView": 1,
                    "componentType": 5125,
                    "count": 6,
                    "type": "SCALAR"
                }}
            ]
        }}"#
    );
    fs::write(&source, json)?;
    Ok(source)
}

#[allow(
    clippy::too_many_lines,
    reason = "the off-mesh fixture spells out both primitive accessor layouts"
)]
fn write_link_source(
    directory: &TempDir,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let mut buffer = Vec::new();
    for value in [
        0.0_f32, 0.0, 0.0, 0.0, 0.0, 4.0, 4.0, 0.0, 4.0, 4.0, 0.0, 0.0, 6.0, 0.0, 0.0, 6.0, 0.0,
        4.0, 10.0, 0.0, 4.0, 10.0, 0.0, 0.0,
    ] {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    for index in [0_u32, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7] {
        buffer.extend_from_slice(&index.to_le_bytes());
    }
    for value in [3.0_f32, 0.0, 2.0, 7.0, 0.0, 2.0] {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    for index in [0_u32, 1] {
        buffer.extend_from_slice(&index.to_le_bytes());
    }
    fs::write(directory.path().join("links.bin"), &buffer)?;
    let source = directory.path().join("links.gltf");
    fs::write(
        &source,
        r#"{
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0, 1]}],
            "nodes": [
                {
                    "name": "Platforms",
                    "mesh": 0,
                    "extras": {"blackflower": {
                        "schema": 1,
                        "node": {"kind": "navigation_surface", "id": "platforms"},
                        "navigation": {"role": "surface", "area_key": "ground"}
                    }}
                },
                {
                    "name": "Gap Link",
                    "mesh": 1,
                    "extras": {"blackflower": {
                        "schema": 1,
                        "node": {"kind": "navigation_off_mesh_link", "id": "jump_gap"},
                        "navigation": {
                            "role": "off_mesh_link",
                            "area_key": "jump",
                            "direction": "bidirectional",
                            "radius": 1.0
                        }
                    }}
                }
            ],
            "meshes": [
                {"primitives": [{
                    "attributes": {"POSITION": 0},
                    "indices": 1,
                    "mode": 4
                }]},
                {"primitives": [{
                    "attributes": {"POSITION": 2},
                    "indices": 3,
                    "mode": 1
                }]}
            ],
            "buffers": [{"uri": "links.bin", "byteLength": 176}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 96},
                {"buffer": 0, "byteOffset": 96, "byteLength": 48},
                {"buffer": 0, "byteOffset": 144, "byteLength": 24},
                {"buffer": 0, "byteOffset": 168, "byteLength": 8}
            ],
            "accessors": [
                {
                    "bufferView": 0,
                    "componentType": 5126,
                    "count": 8,
                    "type": "VEC3",
                    "min": [0, 0, 0],
                    "max": [10, 0, 4]
                },
                {
                    "bufferView": 1,
                    "componentType": 5125,
                    "count": 12,
                    "type": "SCALAR"
                },
                {
                    "bufferView": 2,
                    "componentType": 5126,
                    "count": 2,
                    "type": "VEC3",
                    "min": [3, 0, 2],
                    "max": [7, 0, 2]
                },
                {
                    "bufferView": 3,
                    "componentType": 5125,
                    "count": 2,
                    "type": "SCALAR"
                }
            ]
        }"#,
    )?;
    Ok(source)
}

#[allow(
    clippy::too_many_lines,
    reason = "the cost fixture declares five transformed object-level navigation regions"
)]
fn write_cost_source(
    directory: &TempDir,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let mut buffer = Vec::new();
    for value in [
        0.0_f32, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0,
    ] {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    for index in [0_u32, 1, 2, 0, 2, 3] {
        buffer.extend_from_slice(&index.to_le_bytes());
    }
    fs::write(directory.path().join("cost.bin"), &buffer)?;
    let regions = [
        ("ground_left", "ground", 8.0, 10.0, 0.0, 0.0),
        ("ground_right", "ground", 8.0, 10.0, 12.0, 0.0),
        ("ground_bottom", "ground", 4.0, 2.0, 8.0, 0.0),
        ("ground_top", "ground", 4.0, 2.0, 8.0, 8.0),
        ("mud_center", "mud", 4.0, 6.0, 8.0, 2.0),
    ];
    let nodes = regions
        .iter()
        .map(|&(identifier, area_key, scale_x, scale_z, x, z)| {
            serde_json::json!({
                "name": identifier,
                "mesh": 0,
                "matrix": [
                    scale_x, 0.0, 0.0, 0.0,
                    0.0, 1.0, 0.0, 0.0,
                    0.0, 0.0, scale_z, 0.0,
                    x, 0.0, z, 1.0
                ],
                "extras": {"blackflower": {
                    "schema": 1,
                    "node": {
                        "kind": "navigation_surface",
                        "id": identifier
                    },
                    "navigation": {
                        "role": "surface",
                        "area_key": area_key
                    }
                }}
            })
        })
        .collect::<Vec<_>>();
    let document = serde_json::json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0, 1, 2, 3, 4]}],
        "nodes": nodes,
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": 0},
            "indices": 1,
            "mode": 4
        }]}],
        "buffers": [{"uri": "cost.bin", "byteLength": 72}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 48},
            {"buffer": 0, "byteOffset": 48, "byteLength": 24}
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 4,
                "type": "VEC3",
                "min": [0, 0, 0],
                "max": [1, 0, 1]
            },
            {
                "bufferView": 1,
                "componentType": 5125,
                "count": 6,
                "type": "SCALAR"
            }
        ]
    });
    let source = directory.path().join("cost.gltf");
    fs::write(&source, serde_json::to_vec(&document)?)?;
    Ok(source)
}
