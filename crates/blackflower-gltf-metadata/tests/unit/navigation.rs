use crate::{Document, Error, NavigationDirection, NavigationRole};

#[test]
fn surface_metadata_is_typed() -> Result<(), Error> {
    let document = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [{
                "name": "Floor",
                "extras": {"blackflower": {
                    "schema": 1,
                    "node": {"kind": "navigation_surface", "id": "floor_main"},
                    "navigation": {"role": "surface", "area_key": "ground"}
                }}
            }]
        }"#,
    )?;
    let metadata = document
        .navigation_metadata("Floor")?
        .ok_or_else(|| Error::NodeNotFound("Floor metadata".to_owned()))?;
    assert_eq!(metadata.identifier(), "floor_main");
    assert_eq!(metadata.role(), NavigationRole::Surface);
    assert_eq!(metadata.area_key(), Some("ground"));
    Ok(())
}

#[test]
fn off_mesh_link_requires_complete_policy() -> Result<(), Error> {
    let document = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [{
                "name": "Jump",
                "extras": {"blackflower": {
                    "schema": 1,
                    "node": {"kind": "navigation_off_mesh_link", "id": "jump_gap"},
                    "navigation": {
                        "role": "off_mesh_link",
                        "area_key": "jump",
                        "direction": "bidirectional",
                        "radius": 0.4
                    }
                }}
            }]
        }"#,
    )?;
    let metadata = document
        .navigation_metadata("Jump")?
        .ok_or_else(|| Error::NodeNotFound("Jump metadata".to_owned()))?;
    assert_eq!(
        metadata.direction(),
        Some(NavigationDirection::Bidirectional)
    );
    assert_eq!(metadata.radius(), Some(0.4));
    Ok(())
}
