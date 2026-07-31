use serde::Deserialize;
use serde_json::Value;

use crate::Error;

/// Current schema for Blackflower metadata attached to a glTF node.
pub const NODE_METADATA_SCHEMA: u32 = 1;

const MAX_NODE_KIND_BYTES: usize = 64;
const MAX_NODE_ID_BYTES: usize = 128;

/// Validated typed identity authored on one glTF node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeMetadata {
    name: String,
    kind: String,
    id: Option<String>,
}

impl NodeMetadata {
    /// Stable glTF node name used to select this metadata.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Lower-snake-case domain type, such as `spawn_point`.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Optional stable identifier within the source asset.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeMetadataFile {
    schema: u32,
    node: NodeFile,
    #[serde(default, rename = "navigation")]
    _navigation: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeFile {
    kind: String,
    #[serde(default)]
    id: Option<String>,
}

pub(crate) fn metadata(root: &Value, name: &str) -> Result<Option<NodeMetadata>, Error> {
    let source = find_node(root, name)?;
    let Some(metadata) = source
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("blackflower"))
    else {
        return Ok(None);
    };
    let file: NodeMetadataFile =
        serde_json::from_value(metadata.clone()).map_err(|source| Error::InvalidNodeMetadata {
            node: name.to_owned(),
            source,
        })?;
    validate_schema(name, file.schema)?;
    validate_kind(name, &file.node.kind)?;
    if let Some(id) = &file.node.id {
        validate_id(name, id)?;
    }
    Ok(Some(NodeMetadata {
        name: name.to_owned(),
        kind: file.node.kind,
        id: file.node.id,
    }))
}

pub(crate) fn find_node<'a>(root: &'a Value, name: &str) -> Result<&'a Value, Error> {
    let Some(nodes) = root.get("nodes") else {
        return Err(Error::NodeNotFound(name.to_owned()));
    };
    let nodes = nodes.as_array().ok_or(Error::InvalidNodes)?;
    let mut matching = nodes
        .iter()
        .map(|node| {
            node.as_object()
                .ok_or(Error::InvalidNodes)
                .map(|object| (node, object.get("name").and_then(Value::as_str)))
        })
        .filter_map(|result| match result {
            Ok((node, Some(candidate))) if candidate == name => Some(Ok(node)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        });
    let Some(found) = matching.next().transpose()? else {
        return Err(Error::NodeNotFound(name.to_owned()));
    };
    if matching.next().transpose()?.is_some() {
        return Err(Error::DuplicateNode(name.to_owned()));
    }
    Ok(found)
}

fn validate_schema(node: &str, schema: u32) -> Result<(), Error> {
    if schema == NODE_METADATA_SCHEMA {
        Ok(())
    } else {
        Err(Error::UnsupportedNodeSchema {
            node: node.to_owned(),
            schema,
        })
    }
}

fn validate_kind(node: &str, kind: &str) -> Result<(), Error> {
    let mut characters = kind.chars();
    let valid_start = characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase());
    let valid_rest = characters.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
    });
    if valid_start && valid_rest && kind.len() <= MAX_NODE_KIND_BYTES {
        Ok(())
    } else {
        Err(Error::InvalidNodeKind(node.to_owned()))
    }
}

fn validate_id(node: &str, id: &str) -> Result<(), Error> {
    if id.is_empty()
        || id.len() > MAX_NODE_ID_BYTES
        || id.trim() != id
        || id.chars().any(char::is_control)
    {
        Err(Error::InvalidNodeId(node.to_owned()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
}
