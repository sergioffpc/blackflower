use crate::{AcousticZoneMetadata, Document, Error, MapNavigationDirection, MapNodeRole};

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the fixture demonstrates the complete schema-1 map boundary"
)]
fn schema_one_map_scene_is_typed_in_node_index_order() -> Result<(), Error> {
    let document = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "scenes": [{"name": "Arena", "nodes": [2, 0, 3, 4]}],
            "buffers": [{
                "uri": "data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "byteLength": 36
            }],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36}],
            "accessors": [{
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [0.0, 0.0, 0.0]
            }],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "nodes": [
                {
                    "name": "North Spawn",
                    "extras": {"blackflower": {
                        "schema": 1,
                        "node": {"id": "base_north", "role": "spawn_point"},
                        "spawn_point": {"set": "players", "weight": 2.0}
                    }}
                },
                {
                    "name": "Jump End",
                    "extras": {"blackflower": {
                        "schema": 1,
                        "node": {"id": "jump_end", "role": "navigation_anchor"},
                        "navigation_anchor": {}
                    }}
                },
                {
                    "name": "Jump Start",
                    "children": [1],
                    "extras": {"blackflower": {
                        "schema": 1,
                        "node": {"id": "jump_start", "role": "navigation_link"},
                        "navigation_link": {
                            "end": "jump_end",
                            "area": "jump",
                            "direction": "one_way",
                            "radius": 0.5
                        }
                    }}
                },
                {
                    "name": "Ground Floor",
                    "extras": {"blackflower": {
                        "schema": 1,
                        "node": {"id": "ground_floor", "role": "acoustic_zone"},
                        "acoustic_zone": {"kind": "identity"}
                    }}
                },
                {
                    "name": "Floor",
                    "mesh": 0,
                    "extras": {"blackflower": {
                        "schema": 1,
                        "node": {"id": "floor_main", "role": "geometry"},
                        "geometry": {
                            "render": true,
                            "collision": true,
                            "navigation": "surface",
                            "acoustic_class": "static"
                        }
                    }}
                }
            ],
            "materials": [{
                "name": "Concrete",
                "extras": {"blackflower": {
                    "schema": 1,
                    "material": {
                        "physics_material": "materials/physics/concrete",
                        "navigation_area": "ground",
                        "acoustic_material": "acoustics/materials/concrete"
                    }
                }}
            }]
        }"#,
    )?;

    let map = document.map_metadata("Arena")?;
    assert_eq!(map.scene(), "Arena");
    assert_eq!(
        map.nodes()
            .iter()
            .map(|node| node.identifier())
            .collect::<Vec<_>>(),
        [
            "base_north",
            "jump_end",
            "jump_start",
            "ground_floor",
            "floor_main"
        ]
    );
    assert!(matches!(
        map.nodes()[2].role(),
        MapNodeRole::NavigationLink(link)
            if link.end() == "jump_end"
                && link.direction() == MapNavigationDirection::OneWay
    ));
    assert!(matches!(
        map.nodes()[3].role(),
        MapNodeRole::AcousticZone(AcousticZoneMetadata::Identity)
    ));
    assert_eq!(
        map.materials()[0].physics_material(),
        Some("materials/physics/concrete")
    );
    assert_eq!(map.materials()[0].navigation_area(), Some("ground"));
    Ok(())
}

#[test]
fn map_scene_and_cross_references_are_strict() -> Result<(), Error> {
    let document = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "scenes": [{"name": "Arena", "nodes": [0]}],
            "nodes": [{
                "extras": {"blackflower": {
                    "schema": 1,
                    "node": {"id": "jump_start", "role": "navigation_link"},
                    "navigation_link": {
                        "end": "missing",
                        "area": "jump",
                        "direction": "bidirectional",
                        "radius": 0.5
                    }
                }}
            }]
        }"#,
    )?;

    assert!(matches!(
        document.map_metadata("Missing"),
        Err(Error::MapSceneNotFound(scene)) if scene == "Missing"
    ));
    assert!(matches!(
        document.map_metadata("Arena"),
        Err(Error::InvalidMapMetadata { reason, .. }) if reason.contains("missing")
    ));
    Ok(())
}

#[test]
fn map_roles_reject_wrong_payloads_and_future_schema() -> Result<(), Error> {
    let wrong_payload = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "scenes": [{"name": "Arena", "nodes": [0]}],
            "nodes": [{
                "extras": {"blackflower": {
                    "schema": 1,
                    "node": {"id": "spawn", "role": "spawn_point"},
                    "navigation_anchor": {}
                }}
            }]
        }"#,
    )?;
    assert!(matches!(
        wrong_payload.map_metadata("Arena"),
        Err(Error::InvalidMapMetadata { .. })
    ));

    let future = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "scenes": [{"name": "Arena", "nodes": [0]}],
            "nodes": [{
                "extras": {"blackflower": {
                    "schema": 2,
                    "node": {"id": "spawn", "role": "spawn_point"},
                    "spawn_point": {"set": "default", "weight": 1.0}
                }}
            }]
        }"#,
    )?;
    assert!(matches!(
        future.map_metadata("Arena"),
        Err(Error::InvalidMapMetadata { reason, .. }) if reason.contains("expected schema 1")
    ));
    Ok(())
}
