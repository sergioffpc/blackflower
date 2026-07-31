use std::collections::{BTreeMap, BTreeSet};

use blackflower_gltf_metadata::{
    Document, NavigationDirection, NavigationMetadata, NavigationRole,
};
use blackflower_navigation::NavigationArea;
use glam::{Mat4, Vec3};
use gltf::mesh::Mode;

use crate::Error;

pub(crate) struct Geometry {
    pub(crate) vertices: Vec<f32>,
    pub(crate) indices: Vec<i32>,
    authored_triangle_areas: Vec<Option<u8>>,
    pub(crate) off_mesh_vertices: Vec<f32>,
    pub(crate) off_mesh_radii: Vec<f32>,
    pub(crate) off_mesh_directions: Vec<u8>,
    pub(crate) off_mesh_areas: Vec<u8>,
    pub(crate) off_mesh_flags: Vec<u16>,
    pub(crate) off_mesh_user_ids: Vec<u32>,
}

pub(crate) struct NativeAreas {
    pub(crate) triangle_areas: Vec<u8>,
    pub(crate) remap: [u8; 64],
    pub(crate) traversable: [u8; 64],
}

#[allow(
    clippy::too_many_lines,
    reason = "source import keeps scene selection, typed metadata, and area policy validation together"
)]
pub(crate) fn import(
    path: &std::path::Path,
    areas: &[NavigationArea],
) -> Result<(Geometry, blake3::Hash), Error> {
    let metadata = Document::open(path).map_err(Error::Metadata)?;
    let parsed = gltf::Gltf::open(path).map_err(Error::Gltf)?;
    let base = path
        .parent()
        .ok_or_else(|| Error::InvalidSource("navigation source path has no parent".to_owned()))?;
    let buffers =
        gltf::import_buffers(&parsed.document, Some(base), parsed.blob).map_err(Error::Gltf)?;
    let source_hash = buffer_hash(&buffers);
    let area_ids = areas
        .iter()
        .map(|area| (area.key().as_str(), area.id()))
        .collect::<BTreeMap<_, _>>();
    let scene = select_scene(&parsed.document)?;
    let mut geometry = Geometry {
        vertices: Vec::new(),
        indices: Vec::new(),
        authored_triangle_areas: Vec::new(),
        off_mesh_vertices: Vec::new(),
        off_mesh_radii: Vec::new(),
        off_mesh_directions: Vec::new(),
        off_mesh_areas: Vec::new(),
        off_mesh_flags: Vec::new(),
        off_mesh_user_ids: Vec::new(),
    };
    let mut state = vec![NodeState::Unvisited; parsed.document.nodes().len()];
    let mut identifiers = BTreeSet::new();
    for node in scene.nodes() {
        visit(
            node,
            Mat4::IDENTITY,
            &buffers,
            &metadata,
            &area_ids,
            &mut state,
            &mut identifiers,
            &mut geometry,
        )?;
    }
    for (&area_id, flag) in geometry
        .off_mesh_areas
        .iter()
        .zip(&mut geometry.off_mesh_flags)
    {
        let area = areas
            .get(usize::from(area_id))
            .ok_or_else(|| Error::InvalidArea("off-mesh area ID is invalid".to_owned()))?;
        *flag = u16::from(area.traversable());
    }
    if geometry.indices.is_empty() {
        return Err(Error::InvalidSource(
            "navigation source contains no marked triangle geometry".to_owned(),
        ));
    }
    Ok((geometry, source_hash))
}

pub(crate) fn native_areas(
    geometry: &Geometry,
    areas: &[NavigationArea],
) -> Result<NativeAreas, Error> {
    let used = geometry
        .authored_triangle_areas
        .iter()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>();
    if used.len() > 63 {
        return Err(Error::InvalidArea(
            "Recast supports at most 63 distinct surface areas in one bake".to_owned(),
        ));
    }
    let mut internal_by_authored = [0_u8; 64];
    let mut remap = [0_u8; 64];
    for (index, authored) in used.into_iter().enumerate() {
        let internal = u8::try_from(index + 1)
            .map_err(|_error| Error::InvalidArea("internal area overflow".to_owned()))?;
        internal_by_authored[usize::from(authored)] = internal;
        remap[usize::from(internal)] = authored;
    }
    let triangle_areas = geometry
        .authored_triangle_areas
        .iter()
        .map(|area| area.map_or(0, |id| internal_by_authored[usize::from(id)]))
        .collect();
    let mut traversable = [0_u8; 64];
    for area in areas {
        traversable[usize::from(area.id())] = u8::from(area.traversable());
    }
    Ok(NativeAreas {
        triangle_areas,
        remap,
        traversable,
    })
}

fn select_scene<'a>(document: &'a gltf::Document) -> Result<gltf::Scene<'a>, Error> {
    if let Some(scene) = document.default_scene() {
        return Ok(scene);
    }
    let mut scenes = document.scenes();
    let scene = scenes
        .next()
        .ok_or_else(|| Error::InvalidSource("navigation glTF contains no scene".to_owned()))?;
    if scenes.next().is_some() {
        return Err(Error::InvalidSource(
            "navigation glTF with multiple scenes must declare a default scene".to_owned(),
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
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "scene traversal carries the validated source and deterministic output state"
)]
fn visit(
    node: gltf::Node<'_>,
    parent_transform: Mat4,
    buffers: &[gltf::buffer::Data],
    document: &Document,
    area_ids: &BTreeMap<&str, u8>,
    states: &mut [NodeState],
    identifiers: &mut BTreeSet<String>,
    geometry: &mut Geometry,
) -> Result<(), Error> {
    match states.get(node.index()).copied() {
        Some(NodeState::Unvisited) => {}
        Some(NodeState::Visiting) => {
            return Err(Error::InvalidSource(
                "navigation scene contains a node cycle".to_owned(),
            ));
        }
        Some(NodeState::Visited) => {
            return Err(Error::InvalidSource(
                "navigation scene node has multiple parents".to_owned(),
            ));
        }
        None => {
            return Err(Error::InvalidSource(
                "navigation scene node index is invalid".to_owned(),
            ));
        }
    }
    states[node.index()] = NodeState::Visiting;
    let local = Mat4::from_cols_array_2d(&node.transform().matrix());
    let world = parent_transform * local;
    if !world.is_finite() {
        return Err(Error::InvalidSource(
            "navigation node has a non-finite transform".to_owned(),
        ));
    }
    if let Some(metadata) = document
        .navigation_metadata_at(node.index())
        .map_err(Error::Metadata)?
    {
        if !identifiers.insert(metadata.identifier().to_owned()) {
            return Err(Error::InvalidSource(format!(
                "duplicate navigation node id `{}`",
                metadata.identifier()
            )));
        }
        cook_node(node.clone(), world, buffers, &metadata, area_ids, geometry)?;
    }
    for child in node.children() {
        visit(
            child,
            world,
            buffers,
            document,
            area_ids,
            states,
            identifiers,
            geometry,
        )?;
    }
    states[node.index()] = NodeState::Visited;
    Ok(())
}

fn cook_node(
    node: gltf::Node<'_>,
    transform: Mat4,
    buffers: &[gltf::buffer::Data],
    metadata: &NavigationMetadata,
    area_ids: &BTreeMap<&str, u8>,
    geometry: &mut Geometry,
) -> Result<(), Error> {
    let mesh = node.mesh().ok_or_else(|| {
        Error::InvalidSource(format!(
            "navigation node `{}` has no mesh geometry",
            metadata.identifier()
        ))
    })?;
    match metadata.role() {
        NavigationRole::Surface | NavigationRole::Obstacle => {
            let area = if metadata.role() == NavigationRole::Surface {
                Some(resolve_area(metadata, area_ids)?)
            } else {
                None
            };
            for primitive in mesh.primitives() {
                append_triangles(
                    &primitive,
                    buffers,
                    transform,
                    area,
                    metadata.identifier(),
                    geometry,
                )?;
            }
        }
        NavigationRole::OffMeshLink => {
            append_off_mesh_link(
                &mesh,
                buffers,
                transform,
                metadata,
                resolve_area(metadata, area_ids)?,
                geometry,
            )?;
        }
    }
    Ok(())
}

fn resolve_area(metadata: &NavigationMetadata, area_ids: &BTreeMap<&str, u8>) -> Result<u8, Error> {
    let key = metadata.area_key().ok_or_else(|| {
        Error::InvalidArea(format!(
            "navigation node `{}` has no area key",
            metadata.identifier()
        ))
    })?;
    area_ids.get(key).copied().ok_or_else(|| {
        Error::InvalidArea(format!(
            "navigation node `{}` references undeclared area `{key}`",
            metadata.identifier()
        ))
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "primitive import validates the complete indexed triangle contract before appending"
)]
fn append_triangles(
    primitive: &gltf::Primitive<'_>,
    buffers: &[gltf::buffer::Data],
    transform: Mat4,
    area: Option<u8>,
    identifier: &str,
    geometry: &mut Geometry,
) -> Result<(), Error> {
    if primitive.mode() != Mode::Triangles {
        return Err(Error::InvalidSource(format!(
            "navigation node `{identifier}` contains a non-triangle primitive"
        )));
    }
    let reader =
        primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
    let positions = reader
        .read_positions()
        .ok_or_else(|| {
            Error::InvalidSource(format!(
                "navigation node `{identifier}` is missing POSITION"
            ))
        })?
        .map(|position| transform.transform_point3(Vec3::from(position)))
        .collect::<Vec<_>>();
    if positions.iter().any(|position| !position.is_finite()) {
        return Err(Error::InvalidSource(format!(
            "navigation node `{identifier}` produces non-finite positions"
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
                "navigation node `{identifier}` has too many vertices"
            ))
        })?;
    if indices.is_empty() || !indices.len().is_multiple_of(3) {
        return Err(Error::InvalidSource(format!(
            "navigation node `{identifier}` has incomplete triangles"
        )));
    }
    if indices
        .iter()
        .any(|&index| usize::try_from(index).map_or(true, |index| index >= positions.len()))
    {
        return Err(Error::InvalidSource(format!(
            "navigation node `{identifier}` contains an invalid index"
        )));
    }
    if transform.determinant() < 0.0 {
        for triangle in indices.chunks_exact_mut(3) {
            triangle.swap(1, 2);
        }
    }
    let base = i32::try_from(geometry.vertices.len() / 3)
        .map_err(|_error| Error::InvalidSource("navigation vertex count exceeds i32".to_owned()))?;
    for position in positions {
        geometry.vertices.extend_from_slice(&position.to_array());
    }
    for index in indices {
        let index = i32::try_from(index)
            .map_err(|_error| Error::InvalidSource("navigation index exceeds i32".to_owned()))?;
        geometry.indices.push(
            base.checked_add(index)
                .ok_or_else(|| Error::InvalidSource("navigation index overflow".to_owned()))?,
        );
    }
    geometry.authored_triangle_areas.extend(std::iter::repeat_n(
        area,
        geometry.indices.len() / 3 - geometry.authored_triangle_areas.len(),
    ));
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "off-mesh import validates geometry and all native connection fields as one unit"
)]
fn append_off_mesh_link(
    mesh: &gltf::Mesh<'_>,
    buffers: &[gltf::buffer::Data],
    transform: Mat4,
    metadata: &NavigationMetadata,
    area: u8,
    geometry: &mut Geometry,
) -> Result<(), Error> {
    let mut primitives = mesh.primitives();
    let primitive = primitives.next().ok_or_else(|| {
        Error::InvalidSource(format!(
            "off-mesh link `{}` contains no primitive",
            metadata.identifier()
        ))
    })?;
    if primitives.next().is_some() || primitive.mode() != Mode::Lines {
        return Err(Error::InvalidSource(format!(
            "off-mesh link `{}` must contain exactly one line primitive",
            metadata.identifier()
        )));
    }
    let reader =
        primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
    let positions = reader
        .read_positions()
        .ok_or_else(|| {
            Error::InvalidSource(format!(
                "off-mesh link `{}` is missing POSITION",
                metadata.identifier()
            ))
        })?
        .map(|position| transform.transform_point3(Vec3::from(position)))
        .collect::<Vec<_>>();
    let indices = reader
        .read_indices()
        .map_or_else(
            || {
                (0..positions.len())
                    .map(u32::try_from)
                    .collect::<Result<Vec<_>, _>>()
            },
            |values| Ok(values.into_u32().collect()),
        )
        .map_err(|_error| Error::InvalidSource("off-mesh link index overflow".to_owned()))?;
    if indices.len() != 2 {
        return Err(Error::InvalidSource(format!(
            "off-mesh link `{}` must contain exactly two indexed endpoints",
            metadata.identifier()
        )));
    }
    for index in indices {
        let endpoint = positions
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .ok_or_else(|| {
                Error::InvalidSource(format!(
                    "off-mesh link `{}` contains an invalid index",
                    metadata.identifier()
                ))
            })?;
        if !endpoint.is_finite() {
            return Err(Error::InvalidSource(format!(
                "off-mesh link `{}` produces a non-finite endpoint",
                metadata.identifier()
            )));
        }
        geometry
            .off_mesh_vertices
            .extend_from_slice(&endpoint.to_array());
    }
    geometry
        .off_mesh_radii
        .push(metadata.radius().ok_or_else(|| {
            Error::InvalidSource(format!(
                "off-mesh link `{}` has no radius",
                metadata.identifier()
            ))
        })?);
    geometry.off_mesh_directions.push(u8::from(
        metadata.direction() == Some(NavigationDirection::Bidirectional),
    ));
    geometry.off_mesh_areas.push(area);
    geometry.off_mesh_flags.push(0);
    let digest = blake3::hash(metadata.identifier().as_bytes());
    let user_id = u32::from_le_bytes(
        digest.as_bytes()[..4]
            .try_into()
            .map_err(|_error| Error::InvalidSource("link ID hash failed".to_owned()))?,
    );
    if user_id == 0 || geometry.off_mesh_user_ids.contains(&user_id) {
        return Err(Error::InvalidSource(format!(
            "off-mesh link `{}` does not have a unique non-zero native ID",
            metadata.identifier()
        )));
    }
    geometry.off_mesh_user_ids.push(user_id);
    Ok(())
}

fn buffer_hash(buffers: &[gltf::buffer::Data]) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();
    hash_field(&mut hasher, b"blackflower.navigation-source-buffers.v1");
    for buffer in buffers {
        hash_field(&mut hasher, &buffer.0);
    }
    hasher.finalize()
}

fn hash_field(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}
