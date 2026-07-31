use crate::{Document, Error};

#[test]
fn typed_node_identity_is_extracted() -> Result<(), Error> {
    let document = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [{
                "name": "North Spawn",
                "extras": {
                    "vendor": {"untouched": true},
                    "blackflower": {
                        "schema": 1,
                        "node": {
                            "kind": "spawn_point",
                            "id": "base_north"
                        }
                    }
                }
            }]
        }"#,
    )?;
    let Some(metadata) = document.node_metadata("North Spawn")? else {
        return Err(Error::NodeNotFound("North Spawn metadata".to_owned()));
    };

    assert_eq!(metadata.name(), "North Spawn");
    assert_eq!(metadata.kind(), "spawn_point");
    assert_eq!(metadata.id(), Some("base_north"));
    Ok(())
}

#[test]
fn node_without_blackflower_metadata_is_untyped() -> Result<(), Error> {
    let document = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [{"name": "Mesh", "extras": {"vendor": true}}]
        }"#,
    )?;

    assert!(document.node_metadata("Mesh")?.is_none());
    Ok(())
}

#[test]
fn node_names_must_select_exactly_one_node() -> Result<(), Error> {
    let document = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [{"name": "Spawn"}, {"name": "Spawn"}]
        }"#,
    )?;

    assert!(matches!(
        document.node_metadata("Missing"),
        Err(Error::NodeNotFound(name)) if name == "Missing"
    ));
    assert!(matches!(
        document.node_metadata("Spawn"),
        Err(Error::DuplicateNode(name)) if name == "Spawn"
    ));
    Ok(())
}

#[test]
fn owned_node_metadata_is_strict_and_versioned() -> Result<(), Error> {
    let unknown_field = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [{
                "name": "Spawn",
                "extras": {
                    "blackflower": {
                        "schema": 1,
                        "node": {"kind": "spawn_point", "unknown": true}
                    }
                }
            }]
        }"#,
    )?;
    assert!(matches!(
        unknown_field.node_metadata("Spawn"),
        Err(Error::InvalidNodeMetadata { .. })
    ));

    let unsupported = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [{
                "name": "Spawn",
                "extras": {
                    "blackflower": {
                        "schema": 99,
                        "node": {"kind": "spawn_point"}
                    }
                }
            }]
        }"#,
    )?;
    assert!(matches!(
        unsupported.node_metadata("Spawn"),
        Err(Error::UnsupportedNodeSchema { schema: 99, .. })
    ));
    Ok(())
}

#[test]
fn invalid_node_identity_is_rejected() -> Result<(), Error> {
    let invalid_kind = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [{
                "name": "Spawn",
                "extras": {
                    "blackflower": {
                        "schema": 1,
                        "node": {"kind": "Spawn Point"}
                    }
                }
            }]
        }"#,
    )?;
    assert!(matches!(
        invalid_kind.node_metadata("Spawn"),
        Err(Error::InvalidNodeKind(node)) if node == "Spawn"
    ));

    let invalid_id = Document::from_bytes(
        br#"{
            "asset": {"version": "2.0"},
            "nodes": [{
                "name": "Spawn",
                "extras": {
                    "blackflower": {
                        "schema": 1,
                        "node": {"kind": "spawn_point", "id": " padded"}
                    }
                }
            }]
        }"#,
    )?;
    assert!(matches!(
        invalid_id.node_metadata("Spawn"),
        Err(Error::InvalidNodeId(node)) if node == "Spawn"
    ));
    Ok(())
}
