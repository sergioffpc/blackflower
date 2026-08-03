use crate::{AcousticGeometryClass, AcousticNodeKind, Document, Error, NavigationRole};

#[test]
fn schema_one_acoustic_nodes_are_typed() -> Result<(), Error> {
    let document = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [
                {
                    "name": "Wall",
                    "extras": {"blackflower": {
                        "schema": 1,
                        "node": {"kind": "acoustic_geometry", "id": "wall_north"},
                        "acoustics": {"kind": "geometry", "class": "static"}
                    }}
                },
                {
                    "name": "Ground Floor Probes",
                    "extras": {"blackflower": {
                        "schema": 1,
                        "node": {
                            "kind": "acoustic_probe_volume",
                            "id": "ground_floor_probes"
                        },
                        "acoustics": {
                            "kind": "probe_volume",
                            "zone": "ground_floor"
                        }
                    }}
                }
            ]
        }"#,
    )?;
    let wall = document
        .acoustic_node_metadata("Wall")?
        .ok_or_else(|| Error::NodeNotFound("Wall metadata".to_owned()))?;
    assert_eq!(
        wall.kind(),
        &AcousticNodeKind::Geometry {
            class: AcousticGeometryClass::Static,
        }
    );
    let probes = document
        .acoustic_node_metadata("Ground Floor Probes")?
        .ok_or_else(|| Error::NodeNotFound("probe metadata".to_owned()))?;
    assert!(matches!(
        probes.kind(),
        AcousticNodeKind::ProbeVolume { zone } if zone == "ground_floor"
    ));
    Ok(())
}

#[test]
fn one_node_can_combine_acoustics_and_navigation() -> Result<(), Error> {
    let document = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [{
                "name": "Floor",
                "extras": {"blackflower": {
                    "schema": 1,
                    "node": {"kind": "acoustic_geometry", "id": "floor"},
                    "acoustics": {"kind": "geometry", "class": "static"},
                    "navigation": {"role": "surface", "area_key": "ground"}
                }}
            }]
        }"#,
    )?;
    assert_eq!(
        document
            .navigation_metadata("Floor")?
            .ok_or_else(|| Error::NodeNotFound("Floor navigation".to_owned()))?
            .role(),
        NavigationRole::Surface
    );
    assert!(document.acoustic_node_metadata("Floor")?.is_some());
    Ok(())
}

#[test]
fn schema_one_acoustic_materials_are_typed() -> Result<(), Error> {
    let document = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "materials": [{
                "name": "Concrete",
                "extras": {"blackflower": {
                    "schema": 1,
                    "acoustics": {"material": "acoustics/materials/concrete"}
                }}
            }]
        }"#,
    )?;
    assert_eq!(
        document
            .acoustic_material_metadata("Concrete")?
            .ok_or_else(|| Error::MaterialNotFound("Concrete metadata".to_owned()))?
            .material(),
        "acoustics/materials/concrete"
    );
    Ok(())
}

#[test]
fn zone_volumes_and_portals_are_typed_and_linked() -> Result<(), Error> {
    let document = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [
                {
                    "name": "Room A",
                    "extras": {"blackflower": {
                        "schema": 1,
                        "node": {"kind": "acoustic_zone_volume", "id": "room_a"},
                        "acoustics": {"kind": "zone_volume"}
                    }}
                },
                {
                    "name": "Doorway",
                    "extras": {"blackflower": {
                        "schema": 1,
                        "node": {"kind": "acoustic_portal", "id": "doorway"},
                        "acoustics": {
                            "kind": "portal",
                            "zone_a": "room_a",
                            "zone_b": "room_b"
                        }
                    }}
                }
            ]
        }"#,
    )?;
    assert!(matches!(
        document
            .acoustic_node_metadata("Room A")?
            .ok_or_else(|| Error::NodeNotFound("Room A metadata".to_owned()))?
            .kind(),
        AcousticNodeKind::ZoneVolume
    ));
    assert!(matches!(
        document
            .acoustic_node_metadata("Doorway")?
            .ok_or_else(|| Error::NodeNotFound("Doorway metadata".to_owned()))?
            .kind(),
        AcousticNodeKind::Portal { zone_a, zone_b }
            if zone_a == "room_a" && zone_b == "room_b"
    ));
    Ok(())
}

#[test]
fn mismatched_kind_and_unportable_material_are_rejected() {
    let mismatched = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [{
                "name": "Wall",
                "extras": {"blackflower": {
                    "schema": 1,
                    "node": {"kind": "mesh", "id": "wall"},
                    "acoustics": {"kind": "geometry", "class": "static"}
                }}
            }]
        }"#,
    );
    assert!(matches!(
        mismatched,
        Err(Error::InvalidAcousticMetadata { .. })
    ));

    let invalid_material = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "materials": [{
                "name": "Concrete",
                "extras": {"blackflower": {
                    "schema": 1,
                    "acoustics": {"material": "../Concrete"}
                }}
            }]
        }"#,
    );
    assert!(matches!(
        invalid_material,
        Err(Error::InvalidAcousticMetadata { .. })
    ));
}
