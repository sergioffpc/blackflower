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
    #[serde(default, rename = "acoustics")]
    _acoustics: Option<serde_json::Value>,
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
#[path = "../tests/unit/node.rs"]
mod tests;
