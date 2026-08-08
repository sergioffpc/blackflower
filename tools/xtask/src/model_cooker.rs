use std::collections::BTreeMap;

use anyhow::{Context, bail};
use blackflower_assets::Bytes;
use blackflower_rendering_models::{
    ModelAttachment, ModelAttachmentKind, ModelNode, NodeTransform, encode_model,
};
use gltf::scene::Transform;

use crate::coordinate_system;
use crate::manifest::{
    AssetSource, LoadedAsset, ModelAttachmentManifest, ModelManifest, Repository,
};

pub(crate) const COOKER_RECIPE: &str = "blackflower-model-cooker-v1";

pub(crate) fn cook(
    source: &LoadedAsset,
    manifest: &ModelManifest,
    repository: &Repository,
) -> anyhow::Result<Bytes> {
    let parsed = gltf::Gltf::open(&source.source_path)
        .with_context(|| format!("failed to parse `{}`", source.source_path.display()))?;
    let scene = select_scene(&parsed.document, &manifest.scene)?;
    let mut hierarchy = Hierarchy::new(parsed.document.nodes().len());
    for root in scene.nodes() {
        hierarchy.visit(root, None)?;
    }
    let attachments = resolve_attachments(
        source,
        manifest,
        repository,
        &hierarchy.nodes,
        &hierarchy.source_nodes,
        &hierarchy.names,
    )?;
    encode_model(&hierarchy.nodes, &attachments)
        .map_err(anyhow::Error::from)
        .context("failed to encode cooked runtime model")
}

fn select_scene<'a>(
    document: &'a gltf::Document,
    selected: &str,
) -> anyhow::Result<gltf::Scene<'a>> {
    let mut matches = document
        .scenes()
        .filter(|scene| scene.name() == Some(selected));
    let scene = matches
        .next()
        .with_context(|| format!("glTF scene `{selected}` does not exist"))?;
    if matches.next().is_some() {
        bail!("glTF scene name `{selected}` is ambiguous");
    }
    Ok(scene)
}

struct Hierarchy {
    states: Vec<NodeState>,
    nodes: Vec<ModelNode>,
    source_nodes: Vec<SourceNode>,
    names: BTreeMap<String, Vec<u32>>,
}

impl Hierarchy {
    fn new(source_node_count: usize) -> Self {
        Self {
            states: vec![NodeState::Unvisited; source_node_count],
            nodes: Vec::new(),
            source_nodes: Vec::new(),
            names: BTreeMap::new(),
        }
    }

    fn visit(&mut self, node: gltf::Node<'_>, parent: Option<u32>) -> anyhow::Result<()> {
        match self.states.get(node.index()).copied() {
            Some(NodeState::Unvisited) => {}
            Some(NodeState::Visiting) => {
                bail!("glTF scene contains a node cycle at index {}", node.index());
            }
            Some(NodeState::Visited) => {
                bail!(
                    "glTF node {} is referenced by more than one parent or scene root",
                    node.index()
                );
            }
            None => bail!("glTF node index {} is outside the document", node.index()),
        }
        if node.camera().is_some() {
            bail!(
                "glTF node {} contains an unsupported camera",
                display_node(&node)
            );
        }
        if node.skin().is_some() {
            bail!(
                "glTF node {} contains an unsupported skin",
                display_node(&node)
            );
        }

        self.states[node.index()] = NodeState::Visiting;
        let model_index =
            u32::try_from(self.nodes.len()).context("model node count exceeds u32")?;
        let name = node.name().map(str::to_owned);
        if let Some(name) = &name {
            self.names
                .entry(name.clone())
                .or_default()
                .push(model_index);
        }
        let transform = transform(node.transform())?;
        self.nodes.push(ModelNode::new(name, parent, transform)?);
        self.source_nodes.push(SourceNode {
            mesh: node.mesh().map(|mesh| mesh.name().map(str::to_owned)),
        });
        for child in node.children() {
            self.visit(child, Some(model_index))?;
        }
        self.states[node.index()] = NodeState::Visited;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum NodeState {
    Unvisited,
    Visiting,
    Visited,
}

struct SourceNode {
    mesh: Option<Option<String>>,
}

fn transform(source: Transform) -> anyhow::Result<NodeTransform> {
    let matrix = match source {
        Transform::Decomposed {
            translation,
            rotation,
            scale,
        } => {
            let rotation = normalize_quaternion(glam::Quat::from_array(rotation))?.to_array();
            Transform::Decomposed {
                translation,
                rotation,
                scale,
            }
            .matrix()
        }
        Transform::Matrix { matrix } => matrix,
    };
    NodeTransform::matrix(
        coordinate_system::matrix_from_gltf(glam::Mat4::from_cols_array_2d(&matrix))
            .to_cols_array(),
    )
    .map_err(anyhow::Error::from)
}

fn normalize_quaternion(rotation: glam::Quat) -> anyhow::Result<glam::Quat> {
    if !rotation.is_finite() {
        bail!("model rotation quaternion contains non-finite data");
    }
    let length_squared = rotation.length_squared();
    if length_squared == 0.0 {
        bail!("model rotation quaternion has zero length");
    }
    let mut normalized = rotation.normalize();
    if quaternion_needs_flip(normalized) {
        normalized = -normalized;
    }
    Ok(glam::Quat::from_array(
        normalized
            .to_array()
            .map(|value| if value == 0.0 { 0.0 } else { value }),
    ))
}

fn quaternion_needs_flip(rotation: glam::Quat) -> bool {
    [rotation.w, rotation.x, rotation.y, rotation.z]
        .into_iter()
        .find(|value| *value != 0.0)
        .is_some_and(f32::is_sign_negative)
}

#[allow(
    clippy::too_many_lines,
    reason = "attachment resolution exhaustively rejects every non-model-attachment asset kind"
)]
fn resolve_attachments(
    source: &LoadedAsset,
    manifest: &ModelManifest,
    repository: &Repository,
    nodes: &[ModelNode],
    source_nodes: &[SourceNode],
    names: &BTreeMap<String, Vec<u32>>,
) -> anyhow::Result<Vec<ModelAttachment>> {
    let mut attachments = Vec::with_capacity(manifest.attachments.len());
    let mut mesh_attachment_by_node = vec![false; nodes.len()];
    for attachment in &manifest.attachments {
        let node = resolve_node(attachment, names)?;
        let target = repository
            .assets
            .get(&attachment.asset)
            .with_context(|| format!("missing attachment asset `{}`", attachment.asset))?;
        let kind = match &target.manifest.source {
            AssetSource::Mesh(mesh) => {
                validate_mesh_attachment(
                    source,
                    attachment,
                    node,
                    target,
                    mesh,
                    source_nodes,
                    &mut mesh_attachment_by_node,
                )?;
                ModelAttachmentKind::Mesh
            }
            AssetSource::Volume(_) => ModelAttachmentKind::Volume,
            AssetSource::Blob(_)
            | AssetSource::Luau(_)
            | AssetSource::Shader(_)
            | AssetSource::Texture(_)
            | AssetSource::Model(_)
            | AssetSource::Skeleton(_)
            | AssetSource::Animation(_)
            | AssetSource::Navigation(_)
            | AssetSource::AudioClip(_)
            | AssetSource::AudioStream(_)
            | AssetSource::SoundEvent(_)
            | AssetSource::AcousticScene(_)
            | AssetSource::AcousticProbes(_)
            | AssetSource::Acoustic(_)
            | AssetSource::AcousticMaterials(_)
            | AssetSource::AcousticTopology(_)
            | AssetSource::AcousticPrefab(_)
            | AssetSource::AcousticSimulation(_)
            | AssetSource::AcousticEmission(_) => {
                bail!(
                    "model attachment `{}` has unsupported kind {:?}",
                    attachment.asset,
                    target.manifest.kind()
                );
            }
        };
        attachments.push(ModelAttachment::new(node, attachment.asset.clone(), kind));
    }
    validate_explicit_mesh_attachments(nodes, source_nodes, &mesh_attachment_by_node)?;
    attachments
        .sort_by(|left, right| (left.node(), left.asset()).cmp(&(right.node(), right.asset())));
    Ok(attachments)
}

fn validate_explicit_mesh_attachments(
    nodes: &[ModelNode],
    source_nodes: &[SourceNode],
    mesh_attachment_by_node: &[bool],
) -> anyhow::Result<()> {
    for (node, source_node) in source_nodes.iter().enumerate() {
        if source_node.mesh.is_some() && !mesh_attachment_by_node[node] {
            bail!(
                "model node {} references glTF geometry without an explicit mesh attachment",
                display_model_node(nodes, node)
            );
        }
    }
    Ok(())
}

fn resolve_node(
    attachment: &ModelAttachmentManifest,
    names: &BTreeMap<String, Vec<u32>>,
) -> anyhow::Result<u32> {
    let matches = names
        .get(&attachment.node)
        .with_context(|| format!("attachment node `{}` does not exist", attachment.node))?;
    if matches.len() != 1 {
        bail!(
            "attachment node name `{}` is ambiguous in the selected scene",
            attachment.node
        );
    }
    matches
        .first()
        .copied()
        .context("attachment node match disappeared")
}

#[allow(
    clippy::too_many_arguments,
    reason = "mesh attachment validation needs both authored and resolved model context"
)]
fn validate_mesh_attachment(
    model_source: &LoadedAsset,
    attachment: &ModelAttachmentManifest,
    node: u32,
    mesh_source: &LoadedAsset,
    mesh: &crate::manifest::MeshManifest,
    source_nodes: &[SourceNode],
    mesh_attachment_by_node: &mut [bool],
) -> anyhow::Result<()> {
    if mesh_source.source_path != model_source.source_path {
        bail!(
            "mesh attachment `{}` must select the model's glTF source",
            attachment.asset
        );
    }
    let node_index = usize::try_from(node).context("model node index does not fit usize")?;
    let source_mesh = source_nodes
        .get(node_index)
        .and_then(|source_node| source_node.mesh.as_ref())
        .with_context(|| {
            format!(
                "mesh attachment `{}` targets node `{}` without a glTF mesh",
                attachment.asset, attachment.node
            )
        })?;
    let source_mesh = source_mesh.as_deref().with_context(|| {
        format!(
            "mesh attachment `{}` targets an unnamed glTF mesh",
            attachment.asset
        )
    })?;
    if source_mesh != mesh.mesh {
        bail!(
            "mesh attachment `{}` selects `{}`, but node `{}` references `{source_mesh}`",
            attachment.asset,
            mesh.mesh,
            attachment.node
        );
    }
    let occupied = mesh_attachment_by_node
        .get_mut(node_index)
        .context("model node index is outside the hierarchy")?;
    if *occupied {
        bail!(
            "model node `{}` has more than one mesh attachment",
            attachment.node
        );
    }
    *occupied = true;
    Ok(())
}

fn display_node(node: &gltf::Node<'_>) -> String {
    node.name()
        .map_or_else(|| format!("#{}", node.index()), |name| format!("`{name}`"))
}

fn display_model_node(nodes: &[ModelNode], index: usize) -> String {
    nodes
        .get(index)
        .and_then(ModelNode::name)
        .map_or_else(|| format!("#{index}"), |name| format!("`{name}`"))
}

#[cfg(test)]
#[path = "../tests/unit/model_cooker.rs"]
mod tests;
