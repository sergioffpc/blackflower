use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;

use crate::Error;

/// Current schema for Blackflower map node and material metadata.
pub const MAP_METADATA_SCHEMA: u32 = 1;

const MAX_NODE_ID_BYTES: usize = 128;
const MAX_AREA_KEY_BYTES: usize = 64;
const MAX_ASSET_ID_BYTES: usize = 255;

/// Typed metadata selected from one named glTF map scene.
#[derive(Debug, Clone, PartialEq)]
pub struct MapMetadata {
    scene: String,
    nodes: Vec<MapNodeMetadata>,
    materials: Vec<MapMaterialMetadata>,
}

impl MapMetadata {
    /// Exact glTF scene selected by `map.toml`.
    #[must_use]
    pub fn scene(&self) -> &str {
        &self.scene
    }

    /// Typed map nodes in stable glTF node-index order.
    #[must_use]
    pub fn nodes(&self) -> &[MapNodeMetadata] {
        &self.nodes
    }

    /// Typed Blackflower materials in stable glTF material-index order.
    #[must_use]
    pub fn materials(&self) -> &[MapMaterialMetadata] {
        &self.materials
    }
}

/// One validated node participating in the selected map scene.
#[derive(Debug, Clone, PartialEq)]
pub struct MapNodeMetadata {
    node_index: usize,
    name: Option<String>,
    identifier: String,
    role: MapNodeRole,
}

impl MapNodeMetadata {
    /// Stable glTF node index used to recover geometry and transforms.
    #[must_use]
    pub const fn node_index(&self) -> usize {
        self.node_index
    }

    /// Optional author-facing glTF node name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Stable lower-snake-case map-local identifier.
    #[must_use]
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Single primary role and its complete typed payload.
    #[must_use]
    pub const fn role(&self) -> &MapNodeRole {
        &self.role
    }
}

/// Single primary role assigned to a map node.
#[derive(Debug, Clone, PartialEq)]
pub enum MapNodeRole {
    /// Geometry with independently enabled domain projections.
    Geometry(GeometryMetadata),
    /// Named spawn transform.
    SpawnPoint(SpawnPointMetadata),
    /// Instance of a prefab asset.
    PrefabInstance(AssetInstanceMetadata),
    /// Instance of a volume asset.
    VolumeInstance(AssetInstanceMetadata),
    /// Mesh volume bound to a trigger definition.
    TriggerVolume(TriggerVolumeMetadata),
    /// Empty transform used as a navigation endpoint.
    NavigationAnchor,
    /// Connection from this node to a named navigation anchor.
    NavigationLink(NavigationLinkMetadata),
    /// Acoustic zone identity, bounds, or probe bounds.
    AcousticZone(AcousticZoneMetadata),
    /// Mesh portal connecting two acoustic zone bounds.
    AcousticPortal(AcousticPortalMetadata),
    /// Placed audio source.
    AudioEmitter(AudioEmitterMetadata),
}

/// Combined domain uses for one geometry node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometryMetadata {
    render: bool,
    collision: bool,
    navigation: GeometryNavigation,
    acoustic_class: AcousticGeometryClass,
}

impl GeometryMetadata {
    /// Whether the presentation projection consumes the geometry.
    #[must_use]
    pub const fn render(&self) -> bool {
        self.render
    }

    /// Whether the simulation collision projection consumes the geometry.
    #[must_use]
    pub const fn collision(&self) -> bool {
        self.collision
    }

    /// Navigation use for the geometry.
    #[must_use]
    pub const fn navigation(&self) -> GeometryNavigation {
        self.navigation
    }

    /// Acoustic use for the geometry.
    #[must_use]
    pub const fn acoustic_class(&self) -> AcousticGeometryClass {
        self.acoustic_class
    }
}

/// Navigation participation of a geometry node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryNavigation {
    /// Excluded from navigation cooking.
    None,
    /// Rasterized as walkable input.
    Surface,
    /// Rasterized as blocked input.
    Obstacle,
}

/// Acoustic participation of a geometry node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcousticGeometryClass {
    /// Excluded from acoustic cooking.
    Ignored,
    /// Included in the static acoustic scene.
    Static,
    /// Selected as rigid dynamic geometry by a prefab.
    DynamicRigid,
    /// Selected through state-dependent prefab variants.
    DynamicState,
}

/// Selection policy for a spawn transform.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnPointMetadata {
    set: String,
    weight: f32,
}

impl SpawnPointMetadata {
    /// Stable spawn set key.
    #[must_use]
    pub fn set(&self) -> &str {
        &self.set
    }

    /// Positive relative selection weight.
    #[must_use]
    pub const fn weight(&self) -> f32 {
        self.weight
    }
}

/// Portable asset reference for a prefab or volume instance.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetInstanceMetadata {
    asset: String,
}

impl AssetInstanceMetadata {
    /// Portable referenced asset ID.
    #[must_use]
    pub fn asset(&self) -> &str {
        &self.asset
    }
}

/// Trigger definition attached to a mesh volume.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerVolumeMetadata {
    definition: String,
}

impl TriggerVolumeMetadata {
    /// Portable trigger definition asset ID.
    #[must_use]
    pub fn definition(&self) -> &str {
        &self.definition
    }
}

/// Direction of a navigation link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapNavigationDirection {
    /// Travel from the link node to its end anchor only.
    OneWay,
    /// Travel in both directions.
    Bidirectional,
}

/// Authored navigation connection policy.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavigationLinkMetadata {
    end: String,
    area: String,
    direction: MapNavigationDirection,
    radius: f32,
}

impl NavigationLinkMetadata {
    /// Target navigation-anchor ID.
    #[must_use]
    pub fn end(&self) -> &str {
        &self.end
    }

    /// Navigation area key.
    #[must_use]
    pub fn area(&self) -> &str {
        &self.area
    }

    /// Direction policy.
    #[must_use]
    pub const fn direction(&self) -> MapNavigationDirection {
        self.direction
    }

    /// Positive endpoint matching radius in world units.
    #[must_use]
    pub const fn radius(&self) -> f32 {
        self.radius
    }
}

/// Acoustic-zone purpose of a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcousticZoneMetadata {
    /// Stable zone identity.
    Identity,
    /// Mesh bounds used for authoritative broad phase.
    Bounds,
    /// Mesh bounds in which probes may be generated for a zone identity.
    Probes {
        /// Referenced acoustic-zone identity.
        zone: String,
    },
}

/// Portal policy connecting two zone-bound nodes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticPortalMetadata {
    zone_a: String,
    zone_b: String,
    #[serde(default)]
    controller: Option<String>,
    initially_open: bool,
}

impl AcousticPortalMetadata {
    /// First acoustic-zone bounds ID.
    #[must_use]
    pub fn zone_a(&self) -> &str {
        &self.zone_a
    }

    /// Second acoustic-zone bounds ID.
    #[must_use]
    pub fn zone_b(&self) -> &str {
        &self.zone_b
    }

    /// Optional geometry or prefab-instance node controlling the portal.
    #[must_use]
    pub fn controller(&self) -> Option<&str> {
        self.controller.as_deref()
    }

    /// Initial authoritative portal state.
    #[must_use]
    pub const fn initially_open(&self) -> bool {
        self.initially_open
    }
}

/// Placed audio source policy.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioEmitterMetadata {
    sound: String,
    autoplay: bool,
}

impl AudioEmitterMetadata {
    /// Portable audio asset ID.
    #[must_use]
    pub fn sound(&self) -> &str {
        &self.sound
    }

    /// Whether playback starts when the map becomes active.
    #[must_use]
    pub const fn autoplay(&self) -> bool {
        self.autoplay
    }
}

/// Typed material mappings authored in Blender.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapMaterialMetadata {
    material_index: usize,
    name: Option<String>,
    physics_material: Option<String>,
    navigation_area: Option<String>,
    acoustic_material: Option<String>,
}

impl MapMaterialMetadata {
    /// Stable glTF material index.
    #[must_use]
    pub const fn material_index(&self) -> usize {
        self.material_index
    }

    /// Optional author-facing material name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Optional physics material asset ID.
    #[must_use]
    pub fn physics_material(&self) -> Option<&str> {
        self.physics_material.as_deref()
    }

    /// Optional navigation area key.
    #[must_use]
    pub fn navigation_area(&self) -> Option<&str> {
        self.navigation_area.as_deref()
    }

    /// Optional acoustic material asset ID.
    #[must_use]
    pub fn acoustic_material(&self) -> Option<&str> {
        self.acoustic_material.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RoleFile {
    Geometry,
    SpawnPoint,
    PrefabInstance,
    VolumeInstance,
    TriggerVolume,
    NavigationAnchor,
    NavigationLink,
    AcousticZone,
    AcousticPortal,
    AudioEmitter,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeIdentityFile {
    id: String,
    role: RoleFile,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapNodeFile {
    schema: u32,
    node: NodeIdentityFile,
    #[serde(default)]
    geometry: Option<GeometryMetadata>,
    #[serde(default)]
    spawn_point: Option<SpawnPointMetadata>,
    #[serde(default)]
    prefab_instance: Option<AssetInstanceMetadata>,
    #[serde(default)]
    volume_instance: Option<AssetInstanceMetadata>,
    #[serde(default)]
    trigger_volume: Option<TriggerVolumeMetadata>,
    #[serde(default)]
    navigation_anchor: Option<EmptyPayload>,
    #[serde(default)]
    navigation_link: Option<NavigationLinkMetadata>,
    #[serde(default)]
    acoustic_zone: Option<AcousticZoneFile>,
    #[serde(default)]
    acoustic_portal: Option<AcousticPortalMetadata>,
    #[serde(default)]
    audio_emitter: Option<AudioEmitterMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyPayload {}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum AcousticZoneFile {
    Identity,
    Bounds,
    Probes { zone: String },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapMaterialFile {
    schema: u32,
    material: MaterialPayloadFile,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialPayloadFile {
    #[serde(default)]
    physics_material: Option<String>,
    #[serde(default)]
    navigation_area: Option<String>,
    #[serde(default)]
    acoustic_material: Option<String>,
}

pub(crate) fn metadata(root: &Value, scene_name: &str) -> Result<MapMetadata, Error> {
    let selected = scene_nodes(root, scene_name)?;
    let nodes = root
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or(Error::InvalidNodes)?;
    let mut parsed = Vec::new();
    for index in selected {
        let source = &nodes[index];
        if blackflower(source).is_some() {
            parsed.push(parse_node(source, index)?);
        }
    }
    validate_references(&parsed)?;
    let materials = parse_materials(root)?;
    Ok(MapMetadata {
        scene: scene_name.to_owned(),
        nodes: parsed,
        materials,
    })
}

fn parse_node(source: &Value, index: usize) -> Result<MapNodeMetadata, Error> {
    let name = source
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let owner = name.clone().unwrap_or_else(|| format!("#{index}"));
    let metadata = blackflower(source)
        .ok_or_else(|| invalid(&owner, "map node has no Blackflower metadata"))?;
    let raw: MapNodeFile = serde_json::from_value(metadata.clone())
        .map_err(|error| invalid(&owner, error.to_string()))?;
    if raw.schema != MAP_METADATA_SCHEMA {
        return Err(invalid(
            &owner,
            format!("schema {} is unsupported; expected schema 1", raw.schema),
        ));
    }
    validate_key(&raw.node.id, MAX_NODE_ID_BYTES)
        .then_some(())
        .ok_or_else(|| invalid(&owner, "map node ID is not portable lower_snake_case"))?;
    let has_mesh = source.get("mesh").is_some_and(Value::is_number);
    let role = parse_role(&owner, raw, has_mesh)?;
    Ok(MapNodeMetadata {
        node_index: index,
        name,
        identifier: role.0,
        role: role.1,
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "the closed map-role dispatch validates every payload in one auditable table"
)]
fn parse_role(
    owner: &str,
    raw: MapNodeFile,
    has_mesh: bool,
) -> Result<(String, MapNodeRole), Error> {
    let id = raw.node.id;
    let payload_count = [
        raw.geometry.is_some(),
        raw.spawn_point.is_some(),
        raw.prefab_instance.is_some(),
        raw.volume_instance.is_some(),
        raw.trigger_volume.is_some(),
        raw.navigation_anchor.is_some(),
        raw.navigation_link.is_some(),
        raw.acoustic_zone.is_some(),
        raw.acoustic_portal.is_some(),
        raw.audio_emitter.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if payload_count != 1 {
        return Err(invalid(
            owner,
            "map node must contain exactly its role payload",
        ));
    }
    let (requires_mesh, role) = match raw.node.role {
        RoleFile::Geometry => {
            let value = required(owner, "geometry", raw.geometry)?;
            if !value.render
                && !value.collision
                && value.navigation == GeometryNavigation::None
                && value.acoustic_class == AcousticGeometryClass::Ignored
            {
                return Err(invalid(
                    owner,
                    "geometry must enable at least one domain use",
                ));
            }
            (true, MapNodeRole::Geometry(value))
        }
        RoleFile::SpawnPoint => (
            false,
            MapNodeRole::SpawnPoint(required(owner, "spawn_point", raw.spawn_point)?),
        ),
        RoleFile::PrefabInstance => {
            let value = required(owner, "prefab_instance", raw.prefab_instance)?;
            validate_asset(owner, &value.asset)?;
            (false, MapNodeRole::PrefabInstance(value))
        }
        RoleFile::VolumeInstance => {
            let value = required(owner, "volume_instance", raw.volume_instance)?;
            validate_asset(owner, &value.asset)?;
            (false, MapNodeRole::VolumeInstance(value))
        }
        RoleFile::TriggerVolume => {
            let value = required(owner, "trigger_volume", raw.trigger_volume)?;
            validate_asset(owner, &value.definition)?;
            (true, MapNodeRole::TriggerVolume(value))
        }
        RoleFile::NavigationAnchor => {
            let _payload = required(owner, "navigation_anchor", raw.navigation_anchor)?;
            (false, MapNodeRole::NavigationAnchor)
        }
        RoleFile::NavigationLink => {
            let value = required(owner, "navigation_link", raw.navigation_link)?;
            if !validate_key(&value.end, MAX_NODE_ID_BYTES)
                || !validate_key(&value.area, MAX_AREA_KEY_BYTES)
                || !value.radius.is_finite()
                || value.radius <= 0.0
            {
                return Err(invalid(owner, "navigation link policy is invalid"));
            }
            (false, MapNodeRole::NavigationLink(value))
        }
        RoleFile::AcousticZone => {
            let value = required(owner, "acoustic_zone", raw.acoustic_zone)?;
            let (requires_mesh, value) = match value {
                AcousticZoneFile::Identity => (false, AcousticZoneMetadata::Identity),
                AcousticZoneFile::Bounds => (true, AcousticZoneMetadata::Bounds),
                AcousticZoneFile::Probes { zone } => {
                    if !validate_key(&zone, MAX_NODE_ID_BYTES) {
                        return Err(invalid(owner, "probe zone reference is invalid"));
                    }
                    (true, AcousticZoneMetadata::Probes { zone })
                }
            };
            (requires_mesh, MapNodeRole::AcousticZone(value))
        }
        RoleFile::AcousticPortal => {
            let value = required(owner, "acoustic_portal", raw.acoustic_portal)?;
            if !validate_key(&value.zone_a, MAX_NODE_ID_BYTES)
                || !validate_key(&value.zone_b, MAX_NODE_ID_BYTES)
                || value.zone_a == value.zone_b
                || value
                    .controller
                    .as_deref()
                    .is_some_and(|id| !validate_key(id, MAX_NODE_ID_BYTES))
            {
                return Err(invalid(owner, "acoustic portal policy is invalid"));
            }
            (true, MapNodeRole::AcousticPortal(value))
        }
        RoleFile::AudioEmitter => {
            let value = required(owner, "audio_emitter", raw.audio_emitter)?;
            validate_asset(owner, &value.sound)?;
            (false, MapNodeRole::AudioEmitter(value))
        }
    };
    if has_mesh != requires_mesh {
        let expected = if requires_mesh {
            "mesh"
        } else {
            "transform-only"
        };
        return Err(invalid(
            owner,
            format!("map role requires a {expected} node"),
        ));
    }
    if let MapNodeRole::SpawnPoint(spawn) = &role
        && (!validate_key(&spawn.set, MAX_AREA_KEY_BYTES)
            || !spawn.weight.is_finite()
            || spawn.weight <= 0.0)
    {
        return Err(invalid(owner, "spawn point policy is invalid"));
    }
    Ok((id, role))
}

fn required<T>(owner: &str, role: &str, value: Option<T>) -> Result<T, Error> {
    value.ok_or_else(|| invalid(owner, format!("map role `{role}` is missing its payload")))
}

#[allow(
    clippy::too_many_lines,
    reason = "the closed cross-reference table keeps every map role relationship explicit"
)]
fn validate_references(nodes: &[MapNodeMetadata]) -> Result<(), Error> {
    let mut indexed = BTreeMap::new();
    for node in nodes {
        if indexed.insert(node.identifier.as_str(), node).is_some() {
            return Err(invalid(
                &node.identifier,
                "map contains a duplicate node ID",
            ));
        }
    }
    for node in nodes {
        match &node.role {
            MapNodeRole::NavigationLink(link) => {
                require_reference(&indexed, &node.identifier, &link.end, |role| {
                    matches!(role, MapNodeRole::NavigationAnchor)
                })?;
            }
            MapNodeRole::AcousticZone(AcousticZoneMetadata::Probes { zone }) => {
                require_reference(&indexed, &node.identifier, zone, |role| {
                    matches!(
                        role,
                        MapNodeRole::AcousticZone(AcousticZoneMetadata::Identity)
                    )
                })?;
            }
            MapNodeRole::AcousticPortal(portal) => {
                for target in [&portal.zone_a, &portal.zone_b] {
                    require_reference(&indexed, &node.identifier, target, |role| {
                        matches!(
                            role,
                            MapNodeRole::AcousticZone(AcousticZoneMetadata::Bounds)
                        )
                    })?;
                }
                if let Some(controller) = &portal.controller {
                    require_reference(&indexed, &node.identifier, controller, |role| {
                        matches!(
                            role,
                            MapNodeRole::Geometry(_) | MapNodeRole::PrefabInstance(_)
                        )
                    })?;
                }
            }
            MapNodeRole::Geometry(_)
            | MapNodeRole::SpawnPoint(_)
            | MapNodeRole::PrefabInstance(_)
            | MapNodeRole::VolumeInstance(_)
            | MapNodeRole::TriggerVolume(_)
            | MapNodeRole::NavigationAnchor
            | MapNodeRole::AcousticZone(_)
            | MapNodeRole::AudioEmitter(_) => {}
        }
    }
    Ok(())
}

fn require_reference(
    indexed: &BTreeMap<&str, &MapNodeMetadata>,
    owner: &str,
    target: &str,
    predicate: impl FnOnce(&MapNodeRole) -> bool,
) -> Result<(), Error> {
    let Some(node) = indexed.get(target) else {
        return Err(invalid(
            owner,
            format!("references missing map node `{target}`"),
        ));
    };
    if predicate(&node.role) {
        Ok(())
    } else {
        Err(invalid(
            owner,
            format!("references map node `{target}` with an incompatible role"),
        ))
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "material decoding keeps structural and semantic checks adjacent"
)]
fn parse_materials(root: &Value) -> Result<Vec<MapMaterialMetadata>, Error> {
    let Some(materials) = root.get("materials") else {
        return Ok(Vec::new());
    };
    let materials = materials.as_array().ok_or(Error::InvalidMaterials)?;
    let mut parsed = Vec::new();
    for (index, source) in materials.iter().enumerate() {
        let Some(metadata) = blackflower(source) else {
            continue;
        };
        let name = source
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let owner = name.clone().unwrap_or_else(|| format!("#{index}"));
        let raw: MapMaterialFile = serde_json::from_value(metadata.clone())
            .map_err(|error| invalid(&owner, error.to_string()))?;
        if raw.schema != MAP_METADATA_SCHEMA {
            return Err(invalid(
                &owner,
                format!("schema {} is unsupported; expected schema 1", raw.schema),
            ));
        }
        if raw.material.physics_material.is_none()
            && raw.material.navigation_area.is_none()
            && raw.material.acoustic_material.is_none()
        {
            return Err(invalid(&owner, "map material payload is empty"));
        }
        for asset in [
            raw.material.physics_material.as_deref(),
            raw.material.acoustic_material.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_asset(&owner, asset)?;
        }
        if raw
            .material
            .navigation_area
            .as_deref()
            .is_some_and(|area| !validate_key(area, MAX_AREA_KEY_BYTES))
        {
            return Err(invalid(&owner, "navigation area is not portable"));
        }
        parsed.push(MapMaterialMetadata {
            material_index: index,
            name,
            physics_material: raw.material.physics_material,
            navigation_area: raw.material.navigation_area,
            acoustic_material: raw.material.acoustic_material,
        });
    }
    Ok(parsed)
}

#[allow(
    clippy::too_many_lines,
    reason = "scene selection and deterministic descendant traversal form one validation unit"
)]
fn scene_nodes(root: &Value, scene_name: &str) -> Result<Vec<usize>, Error> {
    let scenes = root
        .get("scenes")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::MapSceneNotFound(scene_name.to_owned()))?;
    let matching: Vec<_> = scenes
        .iter()
        .filter(|scene| scene.get("name").and_then(Value::as_str) == Some(scene_name))
        .collect();
    let [scene] = matching.as_slice() else {
        return if matching.is_empty() {
            Err(Error::MapSceneNotFound(scene_name.to_owned()))
        } else {
            Err(Error::DuplicateMapScene(scene_name.to_owned()))
        };
    };
    let nodes = root
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or(Error::InvalidNodes)?;
    let roots = scene
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(scene_name, "map scene must declare root nodes"))?;
    let mut selected = BTreeSet::new();
    let mut pending: Vec<usize> = roots
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .ok_or_else(|| invalid(scene_name, "map scene node index is invalid"))
        })
        .collect::<Result<_, _>>()?;
    while let Some(index) = pending.pop() {
        if !selected.insert(index) {
            continue;
        }
        let node = nodes
            .get(index)
            .ok_or_else(|| invalid(scene_name, "map scene references a missing node"))?;
        if let Some(children) = node.get("children") {
            let children = children
                .as_array()
                .ok_or_else(|| invalid(scene_name, "map node children are invalid"))?;
            for child in children {
                pending.push(
                    child
                        .as_u64()
                        .and_then(|value| usize::try_from(value).ok())
                        .ok_or_else(|| invalid(scene_name, "map child index is invalid"))?,
                );
            }
        }
    }
    Ok(selected.into_iter().collect())
}

fn blackflower(source: &Value) -> Option<&Value> {
    source
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("blackflower"))
}

fn validate_key(value: &str, maximum: usize) -> bool {
    value.len() <= maximum
        && value
            .bytes()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn validate_asset(owner: &str, value: &str) -> Result<(), Error> {
    if !value.is_empty()
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
    {
        Ok(())
    } else {
        Err(invalid(owner, "asset reference is not a portable asset ID"))
    }
}

fn invalid(owner: &str, reason: impl Into<String>) -> Error {
    Error::InvalidMapMetadata {
        owner: owner.to_owned(),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/map.rs"]
mod tests;
