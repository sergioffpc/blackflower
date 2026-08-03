use std::str::FromStr;

use blackflower_assets::AssetId;
use bytes::{BufMut, Bytes, BytesMut};

use crate::Error;

const MAGIC: &[u8; 8] = b"BFMODEL\0";
const FORMAT_VERSION: u32 = 1;
const NO_PARENT: u32 = u32::MAX;

/// Canonical column-major local transform matrix in Blackflower coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeTransform([f32; 16]);

impl NodeTransform {
    /// Creates a validated canonical matrix transform.
    ///
    /// # Errors
    ///
    /// Returns an error when any matrix component is non-finite.
    pub fn matrix(value: [f32; 16]) -> Result<Self, Error> {
        let value = Self(value);
        validate_transform(value, Error::InvalidInput)?;
        Ok(value)
    }

    /// Returns the column-major matrix values.
    #[must_use]
    pub const fn to_cols_array(self) -> [f32; 16] {
        self.0
    }
}

/// One node in parent-before-child depth-first order.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelNode {
    name: Option<String>,
    parent: Option<u32>,
    transform: NodeTransform,
}

impl ModelNode {
    /// Creates one model node.
    ///
    /// Parent ordering is validated when the complete model is encoded.
    ///
    /// # Errors
    ///
    /// Returns an error when the transform is invalid.
    pub fn new(
        name: Option<String>,
        parent: Option<u32>,
        transform: NodeTransform,
    ) -> Result<Self, Error> {
        validate_transform(transform, Error::InvalidInput)?;
        Ok(Self {
            name,
            parent,
            transform,
        })
    }

    /// Optional authored glTF node name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Parent node index, or `None` for a scene root.
    #[must_use]
    pub const fn parent(&self) -> Option<u32> {
        self.parent
    }

    /// Canonical local transform matrix.
    #[must_use]
    pub const fn transform(&self) -> NodeTransform {
        self.transform
    }
}

/// Runtime attachment representation inferred from the referenced asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelAttachmentKind {
    /// Static geometry decoded through [`crate::MeshAsset`].
    Mesh,
    /// NanoVDB volume decoded through `blackflower-rendering-volumes`.
    Volume,
}

/// One asset attached to a model node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAttachment {
    node: u32,
    asset: AssetId,
    kind: ModelAttachmentKind,
}

impl ModelAttachment {
    /// Creates an attachment. Node bounds are checked by [`encode_model`].
    #[must_use]
    pub const fn new(node: u32, asset: AssetId, kind: ModelAttachmentKind) -> Self {
        Self { node, asset, kind }
    }

    /// Canonical node index resolved by the cooker.
    #[must_use]
    pub const fn node(&self) -> u32 {
        self.node
    }

    /// Logical asset resolved through the package catalog.
    #[must_use]
    pub const fn asset(&self) -> &AssetId {
        &self.asset
    }

    /// Runtime representation expected for the referenced asset.
    #[must_use]
    pub const fn kind(&self) -> ModelAttachmentKind {
        self.kind
    }
}

/// Fully decoded runtime model hierarchy and its asset attachments.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelAsset {
    bytes: Bytes,
    nodes: Vec<ModelNode>,
    attachments: Vec<ModelAttachment>,
}

impl ModelAsset {
    /// Decodes and validates authenticated `.bfmodel` bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for an incompatible header, invalid hierarchy,
    /// malformed transform, invalid asset ID, or non-canonical attachments.
    pub fn from_bytes(bytes: Bytes) -> Result<Self, Error> {
        let (nodes, attachments) = decode(&bytes)?;
        Ok(Self {
            bytes,
            nodes,
            attachments,
        })
    }

    /// Nodes in parent-before-child depth-first order.
    #[must_use]
    pub fn nodes(&self) -> &[ModelNode] {
        &self.nodes
    }

    /// Attachments ordered by node index and then logical asset ID.
    #[must_use]
    pub fn attachments(&self) -> &[ModelAttachment] {
        &self.attachments
    }

    /// Original validated model bytes.
    #[must_use]
    pub const fn bytes(&self) -> &Bytes {
        &self.bytes
    }
}

/// Encodes a validated hierarchy and canonical attachment list into `.bfmodel`.
///
/// # Errors
///
/// Returns an error when counts exceed the format, parents do not precede
/// children, transforms are invalid, or attachments are invalid or unsorted.
pub fn encode_model(nodes: &[ModelNode], attachments: &[ModelAttachment]) -> Result<Bytes, Error> {
    validate_model(nodes, attachments, Error::InvalidInput)?;
    let mut output = BytesMut::new();
    output.extend_from_slice(MAGIC);
    output.put_u32_le(FORMAT_VERSION);
    put_len(&mut output, nodes.len(), "node")?;
    put_len(&mut output, attachments.len(), "attachment")?;
    for node in nodes {
        output.put_u32_le(node.parent.unwrap_or(NO_PARENT));
        put_optional_text(&mut output, node.name.as_deref())?;
        put_floats(&mut output, &node.transform.0);
    }
    for attachment in attachments {
        output.put_u32_le(attachment.node);
        output.put_u8(match attachment.kind {
            ModelAttachmentKind::Mesh => 0,
            ModelAttachmentKind::Volume => 1,
        });
        put_text(&mut output, attachment.asset.as_str())?;
    }
    Ok(output.freeze())
}

fn decode(bytes: &[u8]) -> Result<(Vec<ModelNode>, Vec<ModelAttachment>), Error> {
    let mut reader = Reader::new(bytes);
    if reader.bytes(MAGIC.len())? != MAGIC {
        return Err(Error::InvalidAsset("invalid BFMODEL identifier".to_owned()));
    }
    if reader.u32()? != FORMAT_VERSION {
        return Err(Error::InvalidAsset(
            "unsupported BFMODEL version".to_owned(),
        ));
    }
    let node_count = reader.len("node")?;
    let attachment_count = reader.len("attachment")?;
    let mut nodes = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        let parent = match reader.u32()? {
            NO_PARENT => None,
            value => Some(value),
        };
        let name = reader.optional_text()?;
        let transform = reader.transform()?;
        nodes.push(ModelNode {
            name,
            parent,
            transform,
        });
    }
    let mut attachments = Vec::with_capacity(attachment_count);
    for _ in 0..attachment_count {
        let node = reader.u32()?;
        let kind = match reader.u8()? {
            0 => ModelAttachmentKind::Mesh,
            1 => ModelAttachmentKind::Volume,
            value => {
                return Err(Error::InvalidAsset(format!(
                    "unsupported model attachment kind {value}"
                )));
            }
        };
        let asset_text = reader.text()?;
        let asset = AssetId::from_str(&asset_text)
            .map_err(|error| Error::InvalidAsset(error.to_string()))?;
        attachments.push(ModelAttachment { node, asset, kind });
    }
    if !reader.is_empty() {
        return Err(Error::InvalidAsset(
            "trailing bytes after BFMODEL payload".to_owned(),
        ));
    }
    validate_model(&nodes, &attachments, Error::InvalidAsset)?;
    Ok((nodes, attachments))
}

fn validate_model(
    nodes: &[ModelNode],
    attachments: &[ModelAttachment],
    error: fn(String) -> Error,
) -> Result<(), Error> {
    let _node_count = u32::try_from(nodes.len())
        .map_err(|_error| error("node count exceeds the format limit".to_owned()))?;
    let _attachment_count = u32::try_from(attachments.len())
        .map_err(|_error| error("attachment count exceeds the format limit".to_owned()))?;
    validate_nodes(nodes, error)?;
    validate_attachments(nodes.len(), attachments, error)
}

fn validate_nodes(nodes: &[ModelNode], error: fn(String) -> Error) -> Result<(), Error> {
    let mut open_path = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        validate_transform(node.transform, error)?;
        if node
            .parent
            .is_some_and(|parent| usize::try_from(parent).map_or(true, |parent| parent >= index))
        {
            return Err(error(format!("node {index} parent must precede the child")));
        }
        match node.parent {
            None => open_path.clear(),
            Some(parent) => {
                let Some(parent_position) =
                    open_path.iter().position(|candidate| *candidate == parent)
                else {
                    return Err(error(
                        "nodes must form a depth-first ordered forest".to_owned(),
                    ));
                };
                open_path.truncate(parent_position + 1);
            }
        }
        open_path.push(
            u32::try_from(index)
                .map_err(|_error| error("node index exceeds the format limit".to_owned()))?,
        );
    }
    Ok(())
}

fn validate_attachments(
    node_count: usize,
    attachments: &[ModelAttachment],
    error: fn(String) -> Error,
) -> Result<(), Error> {
    let mut mesh_nodes = vec![false; node_count];
    for (index, attachment) in attachments.iter().enumerate() {
        let node = usize::try_from(attachment.node)
            .map_err(|_error| error("attachment node does not fit usize".to_owned()))?;
        if node >= node_count {
            return Err(error(format!(
                "attachment {index} references missing node {}",
                attachment.node
            )));
        }
        if attachment.kind == ModelAttachmentKind::Mesh {
            if mesh_nodes[node] {
                return Err(error(format!(
                    "node {} has more than one mesh attachment",
                    attachment.node
                )));
            }
            mesh_nodes[node] = true;
        }
    }
    for pair in attachments.windows(2) {
        let left = (&pair[0].node, pair[0].asset.as_str());
        let right = (&pair[1].node, pair[1].asset.as_str());
        if left >= right {
            return Err(error(
                "attachments must be uniquely ordered by node and asset ID".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_transform(transform: NodeTransform, error: fn(String) -> Error) -> Result<(), Error> {
    if !transform.0.into_iter().all(f32::is_finite) {
        return Err(error("model matrix contains non-finite data".to_owned()));
    }
    Ok(())
}

fn put_floats(output: &mut BytesMut, values: &[f32]) {
    for value in values {
        output.put_u32_le(value.to_bits());
    }
}

fn put_optional_text(output: &mut BytesMut, value: Option<&str>) -> Result<(), Error> {
    match value {
        Some(value) => {
            output.put_u8(1);
            put_text(output, value)
        }
        None => {
            output.put_u8(0);
            Ok(())
        }
    }
}

fn put_text(output: &mut BytesMut, value: &str) -> Result<(), Error> {
    put_len(output, value.len(), "string byte")?;
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn put_len(output: &mut BytesMut, value: usize, label: &str) -> Result<(), Error> {
    let value = u32::try_from(value)
        .map_err(|_error| Error::InvalidInput(format!("{label} count exceeds u32")))?;
    output.put_u32_le(value);
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    const fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }

    fn bytes(&mut self, amount: usize) -> Result<&'a [u8], Error> {
        let end = self
            .offset
            .checked_add(amount)
            .ok_or_else(|| Error::InvalidAsset("model byte range overflow".to_owned()))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| Error::InvalidAsset("truncated model asset".to_owned()))?;
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        self.bytes(1)?
            .first()
            .copied()
            .ok_or_else(|| Error::InvalidAsset("truncated model asset".to_owned()))
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let raw: [u8; 4] = self
            .bytes(4)?
            .try_into()
            .map_err(|_error| Error::InvalidAsset("truncated model asset".to_owned()))?;
        Ok(u32::from_le_bytes(raw))
    }

    fn len(&mut self, label: &str) -> Result<usize, Error> {
        usize::try_from(self.u32()?)
            .map_err(|_error| Error::InvalidAsset(format!("{label} count does not fit usize")))
    }

    fn text(&mut self) -> Result<String, Error> {
        let length = self.len("string byte")?;
        let value = std::str::from_utf8(self.bytes(length)?)
            .map_err(|_error| Error::InvalidAsset("model string is not UTF-8".to_owned()))?;
        Ok(value.to_owned())
    }

    fn optional_text(&mut self) -> Result<Option<String>, Error> {
        match self.u8()? {
            0 => Ok(None),
            1 => self.text().map(Some),
            value => Err(Error::InvalidAsset(format!(
                "invalid optional model string tag {value}"
            ))),
        }
    }

    fn transform(&mut self) -> Result<NodeTransform, Error> {
        let transform = NodeTransform(self.f32_array()?);
        validate_transform(transform, Error::InvalidAsset)?;
        Ok(transform)
    }

    fn f32_array<const N: usize>(&mut self) -> Result<[f32; N], Error> {
        let mut values = [0.0; N];
        for value in &mut values {
            *value = f32::from_bits(self.u32()?);
        }
        Ok(values)
    }
}

#[cfg(test)]
#[path = "../tests/unit/model.rs"]
mod tests;
