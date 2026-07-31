use serde::Deserialize;
use serde_json::Value;

use crate::Error;
use crate::node::{NODE_METADATA_SCHEMA, find_node};

const MAX_AREA_KEY_BYTES: usize = 64;

/// How authored geometry participates in navigation cooking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationRole {
    /// Triangle geometry that may produce walkable polygons.
    Surface,
    /// Solid triangle geometry that cuts walkable spans.
    Obstacle,
    /// A two-point primitive that produces one Detour off-mesh connection.
    OffMeshLink,
}

/// Direction policy for an authored off-mesh connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationDirection {
    /// Travel is allowed from the first endpoint to the second only.
    OneWay,
    /// Travel is allowed in both directions.
    Bidirectional,
}

/// Validated object-level navigation metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationMetadata {
    identifier: String,
    role: NavigationRole,
    area_key: Option<String>,
    direction: Option<NavigationDirection>,
    radius: Option<f32>,
}

impl NavigationMetadata {
    /// Stable authored node identifier.
    #[must_use]
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Geometry or connection role.
    #[must_use]
    pub const fn role(&self) -> NavigationRole {
        self.role
    }

    /// Semantic area key required by surfaces and links.
    #[must_use]
    pub fn area_key(&self) -> Option<&str> {
        self.area_key.as_deref()
    }

    /// Off-mesh connection direction.
    #[must_use]
    pub const fn direction(&self) -> Option<NavigationDirection> {
        self.direction
    }

    /// Off-mesh endpoint radius in world units.
    #[must_use]
    pub const fn radius(&self) -> Option<f32> {
        self.radius
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataFile {
    schema: u32,
    node: NodeFile,
    navigation: NavigationFile,
    #[serde(default, rename = "acoustics")]
    _acoustics: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeFile {
    kind: String,
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NavigationFile {
    role: NavigationRole,
    #[serde(default)]
    area_key: Option<String>,
    #[serde(default)]
    direction: Option<NavigationDirection>,
    #[serde(default)]
    radius: Option<f32>,
}

pub(crate) fn metadata(root: &Value, name: &str) -> Result<Option<NavigationMetadata>, Error> {
    let source = find_node(root, name)?;
    parse(source, name)
}

pub(crate) fn metadata_at(
    root: &Value,
    node_index: usize,
) -> Result<Option<NavigationMetadata>, Error> {
    let nodes = root
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or(Error::InvalidNodes)?;
    let source = nodes.get(node_index).ok_or(Error::InvalidNodes)?;
    let label = source
        .get("name")
        .and_then(Value::as_str)
        .map_or_else(|| format!("#{node_index}"), str::to_owned);
    parse(source, &label)
}

pub(crate) fn validate_all(root: &Value) -> Result<(), Error> {
    let Some(nodes) = root.get("nodes") else {
        return Ok(());
    };
    let nodes = nodes.as_array().ok_or(Error::InvalidNodes)?;
    for index in 0..nodes.len() {
        let _metadata = metadata_at(root, index)?;
    }
    Ok(())
}

fn parse(source: &Value, name: &str) -> Result<Option<NavigationMetadata>, Error> {
    let Some(metadata) = source
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("blackflower"))
    else {
        return Ok(None);
    };
    if metadata
        .get("navigation")
        .is_none_or(serde_json::Value::is_null)
    {
        return Ok(None);
    }
    let file: MetadataFile =
        serde_json::from_value(metadata.clone()).map_err(|source| Error::InvalidNodeMetadata {
            node: name.to_owned(),
            source,
        })?;
    if file.schema != NODE_METADATA_SCHEMA {
        return Err(Error::UnsupportedNodeSchema {
            node: name.to_owned(),
            schema: file.schema,
        });
    }
    validate_identifier(name, &file.node.id)?;
    if !portable_key(&file.node.kind, 64) {
        return Err(Error::InvalidNodeKind(name.to_owned()));
    }
    validate_navigation(name, &file.navigation)?;
    Ok(Some(NavigationMetadata {
        identifier: file.node.id,
        role: file.navigation.role,
        area_key: file.navigation.area_key,
        direction: file.navigation.direction,
        radius: file.navigation.radius,
    }))
}

fn validate_navigation(node: &str, navigation: &NavigationFile) -> Result<(), Error> {
    match navigation.role {
        NavigationRole::Surface => {
            validate_area(node, navigation.area_key.as_deref())?;
            if navigation.direction.is_some() || navigation.radius.is_some() {
                return Err(invalid(node, "surface metadata contains link-only fields"));
            }
        }
        NavigationRole::Obstacle => {
            if navigation.area_key.is_some()
                || navigation.direction.is_some()
                || navigation.radius.is_some()
            {
                return Err(invalid(node, "obstacle metadata contains unused fields"));
            }
        }
        NavigationRole::OffMeshLink => {
            validate_area(node, navigation.area_key.as_deref())?;
            if navigation.direction.is_none()
                || navigation
                    .radius
                    .is_none_or(|radius| !radius.is_finite() || radius <= 0.0)
            {
                return Err(invalid(
                    node,
                    "off-mesh link requires a direction and positive finite radius",
                ));
            }
        }
    }
    Ok(())
}

fn validate_area(node: &str, area: Option<&str>) -> Result<(), Error> {
    if area.is_some_and(|value| portable_key(value, MAX_AREA_KEY_BYTES)) {
        Ok(())
    } else {
        Err(invalid(node, "navigation area key is not portable"))
    }
}

fn validate_identifier(node: &str, value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(Error::InvalidNodeId(node.to_owned()))
    } else {
        Ok(())
    }
}

fn portable_key(value: &str, maximum: usize) -> bool {
    value.len() <= maximum
        && value
            .bytes()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn invalid(node: &str, message: &str) -> Error {
    Error::InvalidNodeMetadata {
        node: node.to_owned(),
        source: serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    }
}

#[cfg(test)]
mod tests {
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
}
