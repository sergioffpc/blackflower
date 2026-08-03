use anyhow::{Context, bail};
use blackflower_assets::Bytes;
use blackflower_rendering_models::{
    MeshLod, MeshPrimitive, MeshVertex, VertexAttributes, encode_mesh,
};
use gltf::Semantic;
use gltf::mesh::{Mode, Primitive};
use meshopt::{DecodePosition, SimplifyOptions};

use crate::manifest::{LoadedAsset, MeshManifest};
use crate::profile::MeshProfile;
use crate::{coordinate_system, coordinate_system::vector_from_gltf};

pub(crate) const MESHOPT_VERSION: &str = "0.6.2";
pub(crate) const COOKER_RECIPE: &str = "blackflower-mesh-cooker-v1";

#[derive(Debug, Clone, Copy, Default)]
struct CookVertex {
    position: [f32; 3],
    normal: [f32; 3],
    tangent: [f32; 4],
    texcoord_0: [f32; 2],
}

impl DecodePosition for CookVertex {
    fn decode_position(&self) -> [f32; 3] {
        self.position
    }
}

impl From<CookVertex> for MeshVertex {
    fn from(value: CookVertex) -> Self {
        Self {
            position: value.position,
            normal: value.normal,
            tangent: value.tangent,
            texcoord_0: value.texcoord_0,
        }
    }
}

#[derive(Debug)]
struct RawLod {
    vertices: Vec<CookVertex>,
    indices: Vec<u32>,
}

pub(crate) struct CookedMesh {
    pub(crate) bytes: Bytes,
    pub(crate) source_hash: blake3::Hash,
}

pub(crate) fn cook(
    source: &LoadedAsset,
    manifest: &MeshManifest,
    profile: &MeshProfile,
) -> anyhow::Result<CookedMesh> {
    let (document, buffers) = import_source(source)?;
    let source_hash = buffer_dependency_hash(&buffers);
    let mesh = select_mesh(&document, &manifest.mesh)?;
    let mut primitives = Vec::new();
    for primitive in mesh.primitives() {
        primitives.push(
            cook_primitive(&primitive, &buffers, profile)
                .with_context(|| format!("failed to cook primitive {}", primitive.index()))?,
        );
    }
    if primitives.is_empty() {
        bail!("glTF mesh `{}` contains no primitives", manifest.mesh);
    }
    let bytes = encode_mesh(&primitives).context("failed to encode cooked runtime mesh")?;
    Ok(CookedMesh { bytes, source_hash })
}

fn buffer_dependency_hash(buffers: &[gltf::buffer::Data]) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();
    hash_field(&mut hasher, b"blackflower.mesh-source-buffers.v1");
    for buffer in buffers {
        hash_field(&mut hasher, &buffer.0);
    }
    hasher.finalize()
}

fn import_source(
    source: &LoadedAsset,
) -> anyhow::Result<(gltf::Document, Vec<gltf::buffer::Data>)> {
    let parsed = gltf::Gltf::open(&source.source_path)
        .with_context(|| format!("failed to parse `{}`", source.source_path.display()))?;
    let base = source
        .source_path
        .parent()
        .context("glTF source path has no parent")?;
    let buffers = gltf::import_buffers(&parsed.document, Some(base), parsed.blob)
        .with_context(|| format!("failed to import `{}`", source.source_path.display()))?;
    Ok((parsed.document, buffers))
}

fn select_mesh<'a>(document: &'a gltf::Document, name: &str) -> anyhow::Result<gltf::Mesh<'a>> {
    let mut matches = document.meshes().filter(|mesh| mesh.name() == Some(name));
    let mesh = matches
        .next()
        .with_context(|| format!("glTF contains no mesh named `{name}`"))?;
    if matches.next().is_some() {
        bail!("glTF mesh name `{name}` is ambiguous");
    }
    Ok(mesh)
}

fn cook_primitive(
    primitive: &Primitive<'_>,
    buffers: &[gltf::buffer::Data],
    profile: &MeshProfile,
) -> anyhow::Result<MeshPrimitive> {
    validate_primitive_contract(primitive)?;
    let (vertices, attributes) = read_vertices(primitive, buffers)?;
    let reader =
        primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
    let indices = match reader.read_indices() {
        Some(values) => values.into_u32().collect(),
        None => (0..vertices.len())
            .map(u32::try_from)
            .collect::<Result<Vec<_>, _>>()
            .context("vertex count does not fit u32")?,
    };
    validate_indices(&indices, vertices.len())?;
    let lods = generate_lods(vertices, indices, profile)?;
    let material_index = primitive
        .material()
        .index()
        .map(u32::try_from)
        .transpose()
        .context("material index does not fit u32")?;
    MeshPrimitive::new(material_index, attributes, lods)
        .context("generated primitive violates the runtime mesh contract")
}

fn read_vertices(
    primitive: &Primitive<'_>,
    buffers: &[gltf::buffer::Data],
) -> anyhow::Result<(Vec<CookVertex>, VertexAttributes)> {
    let reader =
        primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
    let positions = reader
        .read_positions()
        .context("triangle primitive is missing POSITION")?
        .collect::<Vec<_>>();
    if positions.is_empty() {
        bail!("triangle primitive contains no vertices");
    }
    let mut attributes = VertexAttributes::positions();
    let normals = reader.read_normals().map(Iterator::collect::<Vec<_>>);
    let tangents = reader.read_tangents().map(Iterator::collect::<Vec<_>>);
    let texcoords = reader
        .read_tex_coords(0)
        .map(|values| values.into_f32().collect::<Vec<_>>());
    validate_attribute_count("NORMAL", positions.len(), normals.as_deref())?;
    validate_attribute_count("TANGENT", positions.len(), tangents.as_deref())?;
    validate_attribute_count("TEXCOORD_0", positions.len(), texcoords.as_deref())?;
    if normals.is_some() {
        attributes = attributes.with(VertexAttributes::NORMAL);
    }
    if tangents.is_some() {
        attributes = attributes.with(VertexAttributes::TANGENT);
    }
    if texcoords.is_some() {
        attributes = attributes.with(VertexAttributes::TEXCOORD_0);
    }
    let vertices = assemble_vertices(
        &positions,
        normals.as_deref(),
        tangents.as_deref(),
        texcoords.as_deref(),
    );
    Ok((vertices, attributes))
}

fn validate_primitive_contract(primitive: &Primitive<'_>) -> anyhow::Result<()> {
    if primitive.mode() != Mode::Triangles {
        bail!("only triangle-list glTF primitives are supported");
    }
    if primitive.morph_targets().next().is_some() {
        bail!("static mesh assets do not support morph targets");
    }
    for (semantic, _accessor) in primitive.attributes() {
        match semantic {
            Semantic::Positions
            | Semantic::Normals
            | Semantic::Tangents
            | Semantic::TexCoords(0) => {}
            Semantic::Joints(_) | Semantic::Weights(_) => {
                bail!("static mesh assets do not support skinning attributes");
            }
            Semantic::Extras(_) | Semantic::Colors(_) | Semantic::TexCoords(_) => {
                bail!("unsupported static mesh vertex attribute `{semantic:?}`");
            }
        }
    }
    Ok(())
}

fn validate_attribute_count<T>(
    semantic: &str,
    expected: usize,
    values: Option<&[T]>,
) -> anyhow::Result<()> {
    if values.is_some_and(|values| values.len() != expected) {
        bail!("{semantic} count does not match POSITION count");
    }
    Ok(())
}

fn assemble_vertices(
    positions: &[[f32; 3]],
    normals: Option<&[[f32; 3]]>,
    tangents: Option<&[[f32; 4]]>,
    texcoords: Option<&[[f32; 2]]>,
) -> Vec<CookVertex> {
    positions
        .iter()
        .enumerate()
        .map(|(index, &position)| CookVertex {
            position: vector_from_gltf(position),
            normal: normals.map_or([0.0; 3], |values| vector_from_gltf(values[index])),
            tangent: tangents.map_or([0.0; 4], |values| {
                coordinate_system::tangent_from_gltf(values[index])
            }),
            texcoord_0: texcoords.map_or([0.0; 2], |values| values[index]),
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/mesh_cooker.rs"]
mod tests;

fn validate_indices(indices: &[u32], vertex_count: usize) -> anyhow::Result<()> {
    if indices.is_empty() || !indices.len().is_multiple_of(3) {
        bail!("triangle primitive must contain complete triangles");
    }
    if indices
        .iter()
        .any(|&index| usize::try_from(index).map_or(true, |value| value >= vertex_count))
    {
        bail!("triangle primitive contains an out-of-range index");
    }
    Ok(())
}

fn generate_lods(
    vertices: Vec<CookVertex>,
    indices: Vec<u32>,
    profile: &MeshProfile,
) -> anyhow::Result<Vec<MeshLod>> {
    let base_index_count = indices.len();
    let mut current = optimize(vertices, indices, profile);
    let mut lods = vec![runtime_lod(0.0, &current)?];
    let mut accumulated_error = 0.0_f32;
    for &percent in &profile.lod_triangle_percents {
        let target_count = target_index_count(base_index_count, percent)?;
        if target_count >= current.indices.len() {
            continue;
        }
        let mut result_error = 0.0_f32;
        let options = if profile.lock_borders {
            SimplifyOptions::LockBorder
        } else {
            SimplifyOptions::None
        };
        let indices = meshopt::simplify_decoder(
            &current.indices,
            &current.vertices,
            target_count,
            profile.lod_target_error,
            options,
            Some(&mut result_error),
        );
        if indices.len() < 3 || indices.len() >= current.indices.len() {
            continue;
        }
        accumulated_error += result_error;
        current = optimize(current.vertices.clone(), indices, profile);
        lods.push(runtime_lod(accumulated_error, &current)?);
    }
    Ok(lods)
}

fn target_index_count(base_index_count: usize, percent: u32) -> anyhow::Result<usize> {
    let base_triangles = base_index_count / 3;
    let percent = usize::try_from(percent).context("LOD percentage does not fit usize")?;
    let triangles = base_triangles
        .checked_mul(percent)
        .context("LOD triangle count overflow")?
        / 100;
    Ok(triangles.max(1) * 3)
}

fn optimize(mut vertices: Vec<CookVertex>, mut indices: Vec<u32>, profile: &MeshProfile) -> RawLod {
    meshopt::optimize_vertex_cache_in_place(&mut indices, vertices.len());
    if profile.optimize_overdraw {
        meshopt::optimize_overdraw_in_place_decoder(
            &mut indices,
            &vertices,
            profile.overdraw_threshold,
        );
    }
    let vertex_count = meshopt::optimize_vertex_fetch_in_place(&mut indices, &mut vertices);
    vertices.truncate(vertex_count);
    RawLod { vertices, indices }
}

fn runtime_lod(error: f32, lod: &RawLod) -> anyhow::Result<MeshLod> {
    let vertices = lod.vertices.iter().copied().map(MeshVertex::from).collect();
    MeshLod::new(error, vertices, lod.indices.clone()).map_err(anyhow::Error::from)
}

fn hash_field(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
}
