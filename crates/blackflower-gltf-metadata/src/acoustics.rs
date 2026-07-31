use serde::Deserialize;
use serde_json::Value;

use crate::Error;
use crate::node::{NODE_METADATA_SCHEMA, find_node};

const MAX_ASSET_ID_BYTES: usize = 255;
const MAX_NODE_ID_BYTES: usize = 128;

/// How authored geometry participates in acoustic cooking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcousticGeometryClass {
    /// Immovable geometry included in the Stage 8 scene.
    Static,
    /// Rigid movable geometry reserved for Stage 9.
    DynamicRigid,
    /// State-dependent geometry reserved for Stage 9.
    DynamicState,
    /// Geometry deliberately excluded from acoustics.
    Ignored,
}

/// Acoustic purpose assigned to one authored glTF node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcousticNodeKind {
    /// Geometry classified for static or later dynamic cooking.
    Geometry {
        /// Authored geometry class.
        class: AcousticGeometryClass,
    },
    /// Named acoustic zone.
    Zone,
    /// Volume in which the cooker may generate probes.
    ProbeVolume {
        /// Stable acoustic-zone identifier containing this volume.
        zone: String,
    },
}

/// Validated schema-1 acoustic metadata attached to a glTF node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcousticNodeMetadata {
    name: String,
    identifier: String,
    kind: AcousticNodeKind,
}

impl AcousticNodeMetadata {
    /// glTF node name used for diagnostics and selection.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Stable authored identifier within the source.
    #[must_use]
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Acoustic purpose and policy.
    #[must_use]
    pub const fn kind(&self) -> &AcousticNodeKind {
        &self.kind
    }
}

/// Validated acoustic material reference attached to a glTF material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcousticMaterialMetadata {
    name: String,
    material: String,
}

impl AcousticMaterialMetadata {
    /// glTF material name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Portable asset ID resolved by the acoustic-scene manifest.
    #[must_use]
    pub fn material(&self) -> &str {
        &self.material
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeMetadataFile {
    schema: u32,
    node: NodeFile,
    acoustics: NodeAcousticsFile,
    #[serde(default, rename = "navigation")]
    _navigation: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeFile {
    kind: String,
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum NodeAcousticsFile {
    Geometry { class: AcousticGeometryClass },
    Zone,
    ProbeVolume { zone: String },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialMetadataFile {
    schema: u32,
    acoustics: MaterialAcousticsFile,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialAcousticsFile {
    material: String,
}

pub(crate) fn node_metadata(
    root: &Value,
    name: &str,
) -> Result<Option<AcousticNodeMetadata>, Error> {
    parse_node(find_node(root, name)?, name)
}

pub(crate) fn node_metadata_at(
    root: &Value,
    index: usize,
) -> Result<Option<AcousticNodeMetadata>, Error> {
    let nodes = root
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or(Error::InvalidNodes)?;
    let source = nodes.get(index).ok_or(Error::InvalidNodes)?;
    let label = source
        .get("name")
        .and_then(Value::as_str)
        .map_or_else(|| format!("#{index}"), str::to_owned);
    parse_node(source, &label)
}

pub(crate) fn material_metadata(
    root: &Value,
    name: &str,
) -> Result<Option<AcousticMaterialMetadata>, Error> {
    let materials = root
        .get("materials")
        .and_then(Value::as_array)
        .ok_or(Error::MaterialNotFound(name.to_owned()))?;
    let mut matching = materials
        .iter()
        .filter(|material| material.get("name").and_then(Value::as_str) == Some(name));
    let Some(source) = matching.next() else {
        return Err(Error::MaterialNotFound(name.to_owned()));
    };
    if matching.next().is_some() {
        return Err(Error::DuplicateMaterial(name.to_owned()));
    }
    parse_material(source, name)
}

pub(crate) fn validate_all(root: &Value) -> Result<(), Error> {
    if let Some(nodes) = root.get("nodes") {
        let nodes = nodes.as_array().ok_or(Error::InvalidNodes)?;
        for index in 0..nodes.len() {
            let _metadata = node_metadata_at(root, index)?;
        }
    }
    if let Some(materials) = root.get("materials") {
        let materials = materials.as_array().ok_or(Error::InvalidMaterials)?;
        for (index, material) in materials.iter().enumerate() {
            let label = material
                .get("name")
                .and_then(Value::as_str)
                .map_or_else(|| format!("#{index}"), str::to_owned);
            let _metadata = parse_material(material, &label)?;
        }
    }
    Ok(())
}

fn parse_node(source: &Value, name: &str) -> Result<Option<AcousticNodeMetadata>, Error> {
    let Some(metadata) = blackflower(source) else {
        return Ok(None);
    };
    if metadata
        .get("acoustics")
        .is_none_or(serde_json::Value::is_null)
    {
        return Ok(None);
    }
    let file: NodeMetadataFile = serde_json::from_value(metadata.clone())
        .map_err(|error| invalid(name, error.to_string()))?;
    validate_schema(name, file.schema)?;
    validate_identifier(name, &file.node.id)?;
    let kind = match file.acoustics {
        NodeAcousticsFile::Geometry { class } => {
            require_kind(name, &file.node.kind, "acoustic_geometry")?;
            AcousticNodeKind::Geometry { class }
        }
        NodeAcousticsFile::Zone => {
            require_kind(name, &file.node.kind, "acoustic_zone")?;
            AcousticNodeKind::Zone
        }
        NodeAcousticsFile::ProbeVolume { zone } => {
            require_kind(name, &file.node.kind, "acoustic_probe_volume")?;
            validate_identifier(name, &zone)?;
            AcousticNodeKind::ProbeVolume { zone }
        }
    };
    Ok(Some(AcousticNodeMetadata {
        name: name.to_owned(),
        identifier: file.node.id,
        kind,
    }))
}

fn parse_material(source: &Value, name: &str) -> Result<Option<AcousticMaterialMetadata>, Error> {
    let Some(metadata) = blackflower(source) else {
        return Ok(None);
    };
    if metadata
        .get("acoustics")
        .is_none_or(serde_json::Value::is_null)
    {
        return Ok(None);
    }
    let file: MaterialMetadataFile = serde_json::from_value(metadata.clone())
        .map_err(|error| invalid(name, error.to_string()))?;
    validate_schema(name, file.schema)?;
    if !portable_asset_id(&file.acoustics.material) {
        return Err(invalid(
            name,
            "acoustic material is not a portable asset ID",
        ));
    }
    Ok(Some(AcousticMaterialMetadata {
        name: name.to_owned(),
        material: file.acoustics.material,
    }))
}

fn blackflower(source: &Value) -> Option<&Value> {
    source
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("blackflower"))
}

fn validate_schema(owner: &str, schema: u32) -> Result<(), Error> {
    if schema == NODE_METADATA_SCHEMA {
        Ok(())
    } else {
        Err(invalid(
            owner,
            format!("schema {schema} is unsupported; expected schema 1"),
        ))
    }
}

fn require_kind(owner: &str, actual: &str, expected: &str) -> Result<(), Error> {
    if actual == expected {
        Ok(())
    } else {
        Err(invalid(
            owner,
            format!("node kind must be `{expected}` for this acoustic kind"),
        ))
    }
}

fn validate_identifier(owner: &str, value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > MAX_NODE_ID_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(invalid(owner, "stable identifier is invalid"))
    } else {
        Ok(())
    }
}

fn portable_asset_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ASSET_ID_BYTES
        && value.is_ascii()
        && value.split('/').all(|segment| {
            !segment.is_empty()
                && !matches!(segment, "." | "..")
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b'-')
                })
        })
}

fn invalid(owner: &str, reason: impl Into<String>) -> Error {
    Error::InvalidAcousticMetadata {
        owner: owner.to_owned(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
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
}
