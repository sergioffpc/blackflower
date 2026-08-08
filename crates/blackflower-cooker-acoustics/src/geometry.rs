use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use blackflower_audio_spatial::{AcousticMaterial, AcousticTriangle, ProbeVolumeTransform, Vec3A};
use blackflower_gltf_metadata::{AcousticGeometryClass, AcousticNodeKind, Document};
use glam::{Mat4, Vec3};
use gltf::mesh::Mode;

use crate::{AcousticMaterialDefinition, Error};

pub(crate) struct ImportedScene {
    pub(crate) vertices: Vec<Vec3A>,
    pub(crate) triangles: Vec<AcousticTriangle>,
    pub(crate) material_indices: Vec<u32>,
    pub(crate) materials: Vec<AcousticMaterial>,
}

pub(crate) struct ImportedProbeVolume {
    pub(crate) zone: String,
    pub(crate) transform: ProbeVolumeTransform,
}

pub(crate) struct ImportedTopology {
    pub(crate) zones: Vec<ImportedZone>,
    pub(crate) portals: Vec<ImportedPortal>,
}

pub(crate) struct ImportedZone {
    pub(crate) name: String,
    pub(crate) bounds: blackflower_acoustics::AabbMm,
}

pub(crate) struct ImportedPortal {
    pub(crate) name: String,
    pub(crate) zone_a: String,
    pub(crate) zone_b: String,
    pub(crate) center: blackflower_acoustics::PositionMm,
}

pub(crate) fn import_scene(
    path: &Path,
    definitions: &[AcousticMaterialDefinition],
) -> Result<ImportedScene, Error> {
    let source = Source::open(path)?;
    let material_table = material_table(definitions)?;
    let mut output = ImportedScene {
        vertices: Vec::new(),
        triangles: Vec::new(),
        material_indices: Vec::new(),
        materials: definitions
            .iter()
            .map(AcousticMaterialDefinition::material)
            .collect(),
    };
    let mut states = vec![NodeState::Unvisited; source.gltf.document.nodes().len()];
    let mut identifiers = BTreeSet::new();
    for node in selected_scene(&source.gltf.document)?.nodes() {
        visit_scene_node(
            node,
            Mat4::IDENTITY,
            &source,
            &material_table,
            &mut states,
            &mut identifiers,
            &mut output,
        )?;
    }
    if output.triangles.is_empty() {
        return Err(Error::InvalidSource(
            "acoustic source contains no static triangle geometry".to_owned(),
        ));
    }
    Ok(output)
}

pub(crate) fn import_probe_volume(
    path: &Path,
    volume_id: &str,
) -> Result<ImportedProbeVolume, Error> {
    let (zones, found) = import_layout(path, Some(volume_id))?;
    let volume = found.ok_or_else(|| {
        Error::InvalidSource(format!(
            "acoustic probe volume `{volume_id}` does not exist"
        ))
    })?;
    if !zones.contains(&volume.zone) {
        return Err(Error::InvalidSource(format!(
            "probe volume `{volume_id}` references missing zone `{}`",
            volume.zone
        )));
    }
    Ok(volume)
}

pub(crate) fn import_zone_ids(path: &Path) -> Result<BTreeSet<String>, Error> {
    import_layout(path, None).map(|(zones, _volume)| zones)
}

pub(crate) fn import_selected_geometry(
    path: &Path,
    definitions: &[AcousticMaterialDefinition],
    selected_id: &str,
) -> Result<ImportedScene, Error> {
    let source = Source::open(path)?;
    let material_table = material_table(definitions)?;
    let mut output = ImportedScene {
        vertices: Vec::new(),
        triangles: Vec::new(),
        material_indices: Vec::new(),
        materials: definitions
            .iter()
            .map(AcousticMaterialDefinition::material)
            .collect(),
    };
    let mut states = vec![NodeState::Unvisited; source.gltf.document.nodes().len()];
    let mut identifiers = BTreeSet::new();
    let mut found = false;
    for node in selected_scene(&source.gltf.document)?.nodes() {
        visit_selected_geometry(
            node,
            Mat4::IDENTITY,
            &source,
            &material_table,
            selected_id,
            &mut states,
            &mut identifiers,
            &mut found,
            &mut output,
        )?;
    }
    if !found || output.triangles.is_empty() {
        return Err(Error::InvalidSource(format!(
            "dynamic acoustic node `{selected_id}` is missing or empty"
        )));
    }
    Ok(output)
}

pub(crate) fn import_topology(path: &Path) -> Result<ImportedTopology, Error> {
    let source = Source::open(path)?;
    let mut states = vec![NodeState::Unvisited; source.gltf.document.nodes().len()];
    let mut identifiers = BTreeSet::new();
    let mut output = ImportedTopology {
        zones: Vec::new(),
        portals: Vec::new(),
    };
    for node in selected_scene(&source.gltf.document)?.nodes() {
        visit_topology_node(
            node,
            Mat4::IDENTITY,
            &source,
            &mut states,
            &mut identifiers,
            &mut output,
        )?;
    }
    output
        .zones
        .sort_by(|left, right| left.name.cmp(&right.name));
    output
        .portals
        .sort_by(|left, right| left.name.cmp(&right.name));
    if output.zones.is_empty() {
        return Err(Error::InvalidSource(
            "acoustic topology contains no zone_volume nodes".to_owned(),
        ));
    }
    let zones = output
        .zones
        .iter()
        .map(|zone| zone.name.as_str())
        .collect::<BTreeSet<_>>();
    for portal in &output.portals {
        if !zones.contains(portal.zone_a.as_str()) || !zones.contains(portal.zone_b.as_str()) {
            return Err(Error::InvalidSource(format!(
                "acoustic portal `{}` references a missing zone",
                portal.name
            )));
        }
    }
    Ok(output)
}

#[allow(
    clippy::too_many_arguments,
    reason = "stable hierarchy traversal carries validation and selected geometry state explicitly"
)]
fn visit_selected_geometry(
    node: gltf::Node<'_>,
    parent: Mat4,
    source: &Source,
    material_table: &BTreeMap<&str, u32>,
    selected_id: &str,
    states: &mut [NodeState],
    identifiers: &mut BTreeSet<String>,
    found: &mut bool,
    output: &mut ImportedScene,
) -> Result<(), Error> {
    enter_node(node.index(), states)?;
    let world = world_transform(&node, parent)?;
    if let Some(metadata) = source.metadata.acoustic_node_metadata_at(node.index())? {
        if !identifiers.insert(metadata.identifier().to_owned()) {
            return Err(Error::InvalidSource(format!(
                "duplicate acoustic node ID `{}`",
                metadata.identifier()
            )));
        }
        if metadata.identifier() == selected_id {
            if !matches!(
                metadata.kind(),
                AcousticNodeKind::Geometry {
                    class: AcousticGeometryClass::DynamicRigid
                        | AcousticGeometryClass::DynamicState
                }
            ) {
                return Err(Error::InvalidSource(format!(
                    "selected acoustic node `{selected_id}` is not dynamic geometry"
                )));
            }
            append_static_geometry(
                node.clone(),
                world,
                source,
                material_table,
                metadata.identifier(),
                output,
            )?;
            *found = true;
        }
    }
    for child in node.children() {
        visit_selected_geometry(
            child,
            world,
            source,
            material_table,
            selected_id,
            states,
            identifiers,
            found,
            output,
        )?;
    }
    states[node.index()] = NodeState::Visited;
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "stable hierarchy traversal carries topology validation state explicitly"
)]
fn visit_topology_node(
    node: gltf::Node<'_>,
    parent: Mat4,
    source: &Source,
    states: &mut [NodeState],
    identifiers: &mut BTreeSet<String>,
    output: &mut ImportedTopology,
) -> Result<(), Error> {
    enter_node(node.index(), states)?;
    let world = world_transform(&node, parent)?;
    if let Some(metadata) = source.metadata.acoustic_node_metadata_at(node.index())? {
        if !identifiers.insert(metadata.identifier().to_owned()) {
            return Err(Error::InvalidSource(format!(
                "duplicate acoustic node ID `{}`",
                metadata.identifier()
            )));
        }
        match metadata.kind() {
            AcousticNodeKind::ZoneVolume => output.zones.push(ImportedZone {
                name: metadata.identifier().to_owned(),
                bounds: quantized_mesh_bounds(node.clone(), world, source)?,
            }),
            AcousticNodeKind::Portal { zone_a, zone_b } => output.portals.push(ImportedPortal {
                name: metadata.identifier().to_owned(),
                zone_a: zone_a.clone(),
                zone_b: zone_b.clone(),
                center: quantize_position(world.transform_point3(Vec3::ZERO))?,
            }),
            AcousticNodeKind::Geometry { .. }
            | AcousticNodeKind::Zone
            | AcousticNodeKind::ProbeVolume { .. } => {}
        }
    }
    for child in node.children() {
        visit_topology_node(child, world, source, states, identifiers, output)?;
    }
    states[node.index()] = NodeState::Visited;
    Ok(())
}

fn quantized_mesh_bounds(
    node: gltf::Node<'_>,
    transform: Mat4,
    source: &Source,
) -> Result<blackflower_acoustics::AabbMm, Error> {
    let mesh = node.mesh().ok_or_else(|| {
        Error::InvalidSource("acoustic zone_volume must contain mesh geometry".to_owned())
    })?;
    let mut minimum = blackflower_acoustics::PositionMm::new(i32::MAX, i32::MAX, i32::MAX);
    let mut maximum = blackflower_acoustics::PositionMm::new(i32::MIN, i32::MIN, i32::MIN);
    let mut count = 0_usize;
    for primitive in mesh.primitives() {
        let reader = primitive.reader(|buffer| {
            source
                .buffers
                .get(buffer.index())
                .map(|data| data.0.as_slice())
        });
        let positions = reader.read_positions().ok_or_else(|| {
            Error::InvalidSource("acoustic zone_volume is missing POSITION".to_owned())
        })?;
        for position in positions {
            let point = quantize_position(transform.transform_point3(Vec3::from(position)))?;
            minimum = blackflower_acoustics::PositionMm::new(
                minimum.x.min(point.x),
                minimum.y.min(point.y),
                minimum.z.min(point.z),
            );
            maximum = blackflower_acoustics::PositionMm::new(
                maximum.x.max(point.x),
                maximum.y.max(point.y),
                maximum.z.max(point.z),
            );
            count = count.saturating_add(1);
        }
    }
    if count == 0 {
        return Err(Error::InvalidSource(
            "acoustic zone_volume is empty".to_owned(),
        ));
    }
    blackflower_acoustics::AabbMm::new(minimum, maximum).map_err(Error::from)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "finite glTF metres are range-checked then intentionally quantized to millimetres"
)]
pub(crate) fn quantize_position(value: Vec3) -> Result<blackflower_acoustics::PositionMm, Error> {
    let convert = |component: f32| {
        let millimetres = f64::from(component) * 1_000.0;
        if !millimetres.is_finite()
            || millimetres < f64::from(i32::MIN)
            || millimetres > f64::from(i32::MAX)
        {
            return Err(Error::InvalidSource(
                "acoustic geometry exceeds millimetre coordinate range".to_owned(),
            ));
        }
        Ok(millimetres.round() as i32)
    };
    Ok(blackflower_acoustics::PositionMm::new(
        convert(value.x)?,
        convert(value.y)?,
        convert(value.z)?,
    ))
}

fn import_layout(
    path: &Path,
    selected_id: Option<&str>,
) -> Result<(BTreeSet<String>, Option<ImportedProbeVolume>), Error> {
    let source = Source::open(path)?;
    let mut states = vec![NodeState::Unvisited; source.gltf.document.nodes().len()];
    let mut identifiers = BTreeSet::new();
    let mut zones = BTreeSet::new();
    let mut found = None;
    for node in selected_scene(&source.gltf.document)?.nodes() {
        visit_volume_node(
            node,
            Mat4::IDENTITY,
            &source,
            selected_id,
            &mut states,
            &mut identifiers,
            &mut zones,
            &mut found,
        )?;
    }
    Ok((zones, found))
}

struct Source {
    metadata: Document,
    gltf: gltf::Gltf,
    buffers: Vec<gltf::buffer::Data>,
}

impl Source {
    fn open(path: &Path) -> Result<Self, Error> {
        let metadata = Document::open(path)?;
        let gltf = gltf::Gltf::open(path)?;
        let base = path
            .parent()
            .ok_or_else(|| Error::InvalidSource("acoustic source path has no parent".to_owned()))?;
        let buffers = gltf::import_buffers(&gltf.document, Some(base), gltf.blob.clone())?;
        Ok(Self {
            metadata,
            gltf,
            buffers,
        })
    }
}

fn material_table(
    definitions: &[AcousticMaterialDefinition],
) -> Result<BTreeMap<&str, u32>, Error> {
    if definitions.is_empty() {
        return Err(Error::InvalidSource(
            "acoustic scene declares no materials".to_owned(),
        ));
    }
    let mut table = BTreeMap::new();
    for (index, definition) in definitions.iter().enumerate() {
        let index = u32::try_from(index)
            .map_err(|_error| Error::InvalidSource("too many acoustic materials".to_owned()))?;
        if table.insert(definition.id(), index).is_some() {
            return Err(Error::InvalidSource(format!(
                "duplicate acoustic material `{}`",
                definition.id()
            )));
        }
    }
    Ok(table)
}

fn selected_scene(document: &gltf::Document) -> Result<gltf::Scene<'_>, Error> {
    if let Some(scene) = document.default_scene() {
        return Ok(scene);
    }
    let mut scenes = document.scenes();
    let scene = scenes
        .next()
        .ok_or_else(|| Error::InvalidSource("acoustic glTF contains no scene".to_owned()))?;
    if scenes.next().is_some() {
        return Err(Error::InvalidSource(
            "acoustic glTF with multiple scenes must declare a default scene".to_owned(),
        ));
    }
    Ok(scene)
}

#[derive(Debug, Clone, Copy)]
enum NodeState {
    Unvisited,
    Visiting,
    Visited,
}

#[allow(
    clippy::too_many_arguments,
    reason = "deterministic traversal carries validation and output state explicitly"
)]
fn visit_scene_node(
    node: gltf::Node<'_>,
    parent: Mat4,
    source: &Source,
    material_table: &BTreeMap<&str, u32>,
    states: &mut [NodeState],
    identifiers: &mut BTreeSet<String>,
    output: &mut ImportedScene,
) -> Result<(), Error> {
    enter_node(node.index(), states)?;
    let world = world_transform(&node, parent)?;
    if let Some(metadata) = source.metadata.acoustic_node_metadata_at(node.index())? {
        if !identifiers.insert(metadata.identifier().to_owned()) {
            return Err(Error::InvalidSource(format!(
                "duplicate acoustic node ID `{}`",
                metadata.identifier()
            )));
        }
        if matches!(
            metadata.kind(),
            AcousticNodeKind::Geometry {
                class: AcousticGeometryClass::Static
            }
        ) {
            append_static_geometry(
                node.clone(),
                world,
                source,
                material_table,
                metadata.identifier(),
                output,
            )?;
        }
    }
    for child in node.children() {
        visit_scene_node(
            child,
            world,
            source,
            material_table,
            states,
            identifiers,
            output,
        )?;
    }
    states[node.index()] = NodeState::Visited;
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "volume selection validates the complete authored acoustic hierarchy"
)]
fn visit_volume_node(
    node: gltf::Node<'_>,
    parent: Mat4,
    source: &Source,
    selected_id: Option<&str>,
    states: &mut [NodeState],
    identifiers: &mut BTreeSet<String>,
    zones: &mut BTreeSet<String>,
    found: &mut Option<ImportedProbeVolume>,
) -> Result<(), Error> {
    enter_node(node.index(), states)?;
    let world = world_transform(&node, parent)?;
    if let Some(metadata) = source.metadata.acoustic_node_metadata_at(node.index())? {
        if !identifiers.insert(metadata.identifier().to_owned()) {
            return Err(Error::InvalidSource(format!(
                "duplicate acoustic node ID `{}`",
                metadata.identifier()
            )));
        }
        match metadata.kind() {
            AcousticNodeKind::Zone => {
                zones.insert(metadata.identifier().to_owned());
            }
            AcousticNodeKind::ProbeVolume { zone }
                if selected_id == Some(metadata.identifier()) =>
            {
                if found.is_some() {
                    return Err(Error::InvalidSource(format!(
                        "duplicate selected probe volume `{}`",
                        metadata.identifier()
                    )));
                }
                *found = Some(ImportedProbeVolume {
                    zone: zone.clone(),
                    transform: probe_volume_transform(node.clone(), world, source)?,
                });
            }
            AcousticNodeKind::Geometry { .. }
            | AcousticNodeKind::ProbeVolume { .. }
            | AcousticNodeKind::ZoneVolume
            | AcousticNodeKind::Portal { .. } => {}
        }
    }
    for child in node.children() {
        visit_volume_node(
            child,
            world,
            source,
            selected_id,
            states,
            identifiers,
            zones,
            found,
        )?;
    }
    states[node.index()] = NodeState::Visited;
    Ok(())
}

fn enter_node(index: usize, states: &mut [NodeState]) -> Result<(), Error> {
    match states.get(index).copied() {
        Some(NodeState::Unvisited) => {
            states[index] = NodeState::Visiting;
            Ok(())
        }
        Some(NodeState::Visiting) => Err(Error::InvalidSource(
            "acoustic scene contains a node cycle".to_owned(),
        )),
        Some(NodeState::Visited) => Err(Error::InvalidSource(
            "acoustic scene node has multiple parents".to_owned(),
        )),
        None => Err(Error::InvalidSource(
            "acoustic scene node index is invalid".to_owned(),
        )),
    }
}

fn world_transform(node: &gltf::Node<'_>, parent: Mat4) -> Result<Mat4, Error> {
    let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
    if world.is_finite() {
        Ok(world)
    } else {
        Err(Error::InvalidSource(
            "acoustic node has a non-finite transform".to_owned(),
        ))
    }
}

fn append_static_geometry(
    node: gltf::Node<'_>,
    transform: Mat4,
    source: &Source,
    material_table: &BTreeMap<&str, u32>,
    identifier: &str,
    output: &mut ImportedScene,
) -> Result<(), Error> {
    let mesh = node.mesh().ok_or_else(|| {
        Error::InvalidSource(format!("static acoustic node `{identifier}` has no mesh"))
    })?;
    for primitive in mesh.primitives() {
        if primitive.mode() != Mode::Triangles {
            return Err(Error::InvalidSource(format!(
                "static acoustic node `{identifier}` contains a non-triangle primitive"
            )));
        }
        let material_name = primitive.material().name().ok_or_else(|| {
            Error::InvalidSource(format!(
                "static acoustic node `{identifier}` uses an unnamed material"
            ))
        })?;
        let acoustic = source
            .metadata
            .acoustic_material_metadata(material_name)?
            .ok_or_else(|| {
                Error::InvalidSource(format!(
                    "glTF material `{material_name}` has no acoustic mapping"
                ))
            })?;
        let material_index = material_table
            .get(acoustic.material())
            .copied()
            .ok_or_else(|| {
                Error::InvalidSource(format!(
                    "glTF material `{material_name}` references undeclared acoustic material `{}`",
                    acoustic.material()
                ))
            })?;
        append_primitive(
            &primitive,
            transform,
            &source.buffers,
            material_index,
            identifier,
            output,
        )?;
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "primitive import validates and transforms one complete triangle stream"
)]
fn append_primitive(
    primitive: &gltf::Primitive<'_>,
    transform: Mat4,
    buffers: &[gltf::buffer::Data],
    material_index: u32,
    identifier: &str,
    output: &mut ImportedScene,
) -> Result<(), Error> {
    let reader =
        primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
    let positions = reader
        .read_positions()
        .ok_or_else(|| {
            Error::InvalidSource(format!(
                "static acoustic node `{identifier}` is missing POSITION"
            ))
        })?
        .map(|position| transform.transform_point3(Vec3::from(position)))
        .collect::<Vec<_>>();
    if positions.iter().any(|position| !position.is_finite()) {
        return Err(Error::InvalidSource(format!(
            "static acoustic node `{identifier}` produces non-finite positions"
        )));
    }
    let mut indices = reader
        .read_indices()
        .map_or_else(
            || {
                (0..positions.len())
                    .map(u32::try_from)
                    .collect::<Result<Vec<_>, _>>()
            },
            |values| Ok(values.into_u32().collect()),
        )
        .map_err(|_error| {
            Error::InvalidSource(format!(
                "static acoustic node `{identifier}` has too many vertices"
            ))
        })?;
    if indices.is_empty() || !indices.len().is_multiple_of(3) {
        return Err(Error::InvalidSource(format!(
            "static acoustic node `{identifier}` has incomplete triangles"
        )));
    }
    if indices
        .iter()
        .any(|&index| usize::try_from(index).map_or(true, |index| index >= positions.len()))
    {
        return Err(Error::InvalidSource(format!(
            "static acoustic node `{identifier}` contains an invalid index"
        )));
    }
    if transform.determinant() < 0.0 {
        for triangle in indices.chunks_exact_mut(3) {
            triangle.swap(1, 2);
        }
    }
    let base = u32::try_from(output.vertices.len())
        .map_err(|_error| Error::InvalidSource("acoustic vertex count exceeds u32".to_owned()))?;
    output
        .vertices
        .extend(positions.into_iter().map(Vec3A::from));
    for triangle in indices.chunks_exact(3) {
        let map_index = |index: u32| {
            base.checked_add(index)
                .ok_or_else(|| Error::InvalidSource("acoustic triangle index overflow".to_owned()))
        };
        output.triangles.push(AcousticTriangle::new(
            map_index(triangle[0])?,
            map_index(triangle[1])?,
            map_index(triangle[2])?,
        ));
        output.material_indices.push(material_index);
    }
    Ok(())
}

fn probe_volume_transform(
    node: gltf::Node<'_>,
    world: Mat4,
    source: &Source,
) -> Result<ProbeVolumeTransform, Error> {
    let mesh = node.mesh().ok_or_else(|| {
        Error::InvalidSource("acoustic probe volume must contain mesh geometry".to_owned())
    })?;
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    let mut count = 0_usize;
    for primitive in mesh.primitives() {
        let reader = primitive.reader(|buffer| {
            source
                .buffers
                .get(buffer.index())
                .map(|data| data.0.as_slice())
        });
        let positions = reader.read_positions().ok_or_else(|| {
            Error::InvalidSource("acoustic probe volume is missing POSITION".to_owned())
        })?;
        for position in positions {
            let position = Vec3::from(position);
            if !position.is_finite() {
                return Err(Error::InvalidSource(
                    "acoustic probe volume has non-finite positions".to_owned(),
                ));
            }
            minimum = minimum.min(position);
            maximum = maximum.max(position);
            count = count.saturating_add(1);
        }
    }
    let size = maximum - minimum;
    if count == 0 || !size.is_finite() || size.min_element() <= 0.0 {
        return Err(Error::InvalidSource(
            "acoustic probe volume bounds are empty or degenerate".to_owned(),
        ));
    }
    let center = (minimum + maximum) * 0.5;
    let transform =
        world * Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, center);
    ProbeVolumeTransform::new(transform).map_err(Error::from)
}
