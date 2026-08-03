use bytes::{BufMut, Bytes, BytesMut};

use crate::Error;

const MAGIC: &[u8; 8] = b"BFMESH\0\0";
const FORMAT_VERSION: u32 = 1;
const VERTEX_FLOATS: usize = 12;
const VERTEX_BYTES: usize = VERTEX_FLOATS * size_of::<f32>();
const MAX_PRIMITIVES: usize = 65_535;
const MAX_LODS: usize = 16;

/// Vertex channels carried by every vertex in a primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexAttributes(u32);

impl VertexAttributes {
    /// The required object-space position channel.
    pub const POSITION: Self = Self(1 << 0);
    /// The object-space normal channel.
    pub const NORMAL: Self = Self(1 << 1);
    /// The tangent and handedness channel.
    pub const TANGENT: Self = Self(1 << 2);
    /// The first texture-coordinate channel.
    pub const TEXCOORD_0: Self = Self(1 << 3);

    const KNOWN: u32 = Self::POSITION.0 | Self::NORMAL.0 | Self::TANGENT.0 | Self::TEXCOORD_0.0;

    /// Constructs a channel mask containing only positions.
    #[must_use]
    pub const fn positions() -> Self {
        Self::POSITION
    }

    /// Adds a channel to this mask.
    #[must_use]
    pub const fn with(self, channel: Self) -> Self {
        Self(self.0 | channel.0)
    }

    /// Returns whether this mask contains a channel.
    #[must_use]
    pub const fn contains(self, channel: Self) -> bool {
        self.0 & channel.0 == channel.0
    }

    const fn bits(self) -> u32 {
        self.0
    }

    fn from_bits(bits: u32) -> Result<Self, Error> {
        if bits & !Self::KNOWN != 0 || bits & Self::POSITION.0 == 0 {
            return Err(Error::InvalidAsset(format!(
                "unsupported vertex attribute mask 0x{bits:08x}"
            )));
        }
        Ok(Self(bits))
    }
}

/// Fixed runtime vertex used by the first mesh format revision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVertex {
    /// Object-space position.
    pub position: [f32; 3],
    /// Object-space normal, or zero when the channel is absent.
    pub normal: [f32; 3],
    /// Tangent and handedness, or zero when the channel is absent.
    pub tangent: [f32; 4],
    /// First texture coordinate, or zero when the channel is absent.
    pub texcoord_0: [f32; 2],
}

/// Axis-aligned object-space bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Minimum coordinate on each axis.
    pub min: [f32; 3],
    /// Maximum coordinate on each axis.
    pub max: [f32; 3],
}

/// One independently drawable level of detail.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshLod {
    geometric_error: f32,
    bounds: Bounds,
    vertices: Vec<MeshVertex>,
    indices: Vec<u32>,
}

impl MeshLod {
    /// Builds and validates one triangle-list LOD.
    ///
    /// # Errors
    ///
    /// Returns an error for non-finite data, invalid indices, or an empty
    /// triangle list.
    pub fn new(
        geometric_error: f32,
        vertices: Vec<MeshVertex>,
        indices: Vec<u32>,
    ) -> Result<Self, Error> {
        validate_lod(geometric_error, &vertices, &indices, Error::InvalidInput)?;
        let bounds = calculate_bounds(&vertices);
        Ok(Self {
            geometric_error,
            bounds,
            vertices,
            indices,
        })
    }

    /// Meshoptimizer geometric error accumulated from the base mesh.
    #[must_use]
    pub const fn geometric_error(&self) -> f32 {
        self.geometric_error
    }

    /// Object-space bounds for this LOD.
    #[must_use]
    pub const fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// Vertices in GPU upload order.
    #[must_use]
    pub fn vertices(&self) -> &[MeshVertex] {
        &self.vertices
    }

    /// Triangle-list indices.
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
}

/// One glTF primitive and its generated LOD chain.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshPrimitive {
    material_index: Option<u32>,
    attributes: VertexAttributes,
    lods: Vec<MeshLod>,
}

impl MeshPrimitive {
    /// Builds one primitive with a base LOD followed by coarser LODs.
    ///
    /// # Errors
    ///
    /// Returns an error when the LOD chain is empty, too long, or not strictly
    /// decreasing in triangle count.
    pub fn new(
        material_index: Option<u32>,
        attributes: VertexAttributes,
        lods: Vec<MeshLod>,
    ) -> Result<Self, Error> {
        validate_primitive(attributes, &lods, Error::InvalidInput)?;
        Ok(Self {
            material_index,
            attributes,
            lods,
        })
    }

    /// Source glTF material index retained as a stable material slot.
    #[must_use]
    pub const fn material_index(&self) -> Option<u32> {
        self.material_index
    }

    /// Vertex channels present in the authored primitive.
    #[must_use]
    pub const fn attributes(&self) -> VertexAttributes {
        self.attributes
    }

    /// Base mesh followed by successively coarser LODs.
    #[must_use]
    pub fn lods(&self) -> &[MeshLod] {
        &self.lods
    }
}

/// Fully decoded runtime mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshAsset {
    bytes: Bytes,
    primitives: Vec<MeshPrimitive>,
}

impl MeshAsset {
    /// Decodes and validates authenticated cooked mesh bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the binary structure or mesh data violates the
    /// current mesh format contract.
    pub fn from_bytes(bytes: Bytes) -> Result<Self, Error> {
        let primitives = decode(&bytes)?;
        Ok(Self { bytes, primitives })
    }

    /// Independently drawable primitives in source order.
    #[must_use]
    pub fn primitives(&self) -> &[MeshPrimitive] {
        &self.primitives
    }

    /// Original validated mesh bytes.
    #[must_use]
    pub const fn bytes(&self) -> &Bytes {
        &self.bytes
    }
}

/// Encodes validated primitives into the deterministic runtime mesh format.
///
/// # Errors
///
/// Returns an error when the mesh is empty, exceeds format limits, or
/// contains invalid mesh data.
pub fn encode_mesh(primitives: &[MeshPrimitive]) -> Result<Bytes, Error> {
    if primitives.is_empty() || primitives.len() > MAX_PRIMITIVES {
        return Err(Error::InvalidInput(format!(
            "mesh must contain from 1 through {MAX_PRIMITIVES} primitives"
        )));
    }
    let mut output = BytesMut::new();
    output.extend_from_slice(MAGIC);
    output.put_u32_le(FORMAT_VERSION);
    put_len(&mut output, primitives.len(), "primitive")?;
    for primitive in primitives {
        validate_primitive(primitive.attributes, &primitive.lods, Error::InvalidInput)?;
        output.put_u32_le(primitive.material_index.unwrap_or(u32::MAX));
        output.put_u32_le(primitive.attributes.bits());
        put_len(&mut output, primitive.lods.len(), "LOD")?;
        for lod in &primitive.lods {
            encode_lod(&mut output, lod)?;
        }
    }
    Ok(output.freeze())
}

fn encode_lod(output: &mut BytesMut, lod: &MeshLod) -> Result<(), Error> {
    validate_lod(
        lod.geometric_error,
        &lod.vertices,
        &lod.indices,
        Error::InvalidInput,
    )?;
    output.put_u32_le(lod.geometric_error.to_bits());
    put_floats(output, &lod.bounds.min);
    put_floats(output, &lod.bounds.max);
    put_len(output, lod.vertices.len(), "vertex")?;
    put_len(output, lod.indices.len(), "index")?;
    for vertex in &lod.vertices {
        put_floats(output, &vertex.position);
        put_floats(output, &vertex.normal);
        put_floats(output, &vertex.tangent);
        put_floats(output, &vertex.texcoord_0);
    }
    for &index in &lod.indices {
        output.put_u32_le(index);
    }
    Ok(())
}

fn decode(bytes: &[u8]) -> Result<Vec<MeshPrimitive>, Error> {
    let mut reader = Reader::new(bytes);
    if reader.take(MAGIC.len())? != MAGIC {
        return Err(Error::InvalidAsset("invalid format identifier".to_owned()));
    }
    let version = reader.u32()?;
    if version != FORMAT_VERSION {
        return Err(Error::InvalidAsset(format!(
            "unsupported format version {version}"
        )));
    }
    let primitive_count = reader.count("primitive", MAX_PRIMITIVES)?;
    if primitive_count == 0 {
        return Err(Error::InvalidAsset(
            "mesh contains no primitives".to_owned(),
        ));
    }
    let mut primitives = Vec::with_capacity(primitive_count);
    for _ in 0..primitive_count {
        primitives.push(decode_primitive(&mut reader)?);
    }
    if reader.remaining() != 0 {
        return Err(Error::InvalidAsset(
            "trailing bytes after mesh data".to_owned(),
        ));
    }
    Ok(primitives)
}

fn decode_primitive(reader: &mut Reader<'_>) -> Result<MeshPrimitive, Error> {
    let material = reader.u32()?;
    let attributes = VertexAttributes::from_bits(reader.u32()?)?;
    let lod_count = reader.count("LOD", MAX_LODS)?;
    let mut lods = Vec::with_capacity(lod_count);
    for _ in 0..lod_count {
        lods.push(decode_lod(reader)?);
    }
    validate_primitive(attributes, &lods, Error::InvalidAsset)?;
    Ok(MeshPrimitive {
        material_index: (material != u32::MAX).then_some(material),
        attributes,
        lods,
    })
}

fn decode_lod(reader: &mut Reader<'_>) -> Result<MeshLod, Error> {
    let geometric_error = f32::from_bits(reader.u32()?);
    let bounds = Bounds {
        min: reader.f32_array()?,
        max: reader.f32_array()?,
    };
    let vertex_count = reader.count_for_bytes("vertex", VERTEX_BYTES)?;
    let index_count = reader.count_for_bytes("index", size_of::<u32>())?;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertices.push(MeshVertex {
            position: reader.f32_array()?,
            normal: reader.f32_array()?,
            tangent: reader.f32_array()?,
            texcoord_0: reader.f32_array()?,
        });
    }
    let mut indices = Vec::with_capacity(index_count);
    for _ in 0..index_count {
        indices.push(reader.u32()?);
    }
    validate_lod(geometric_error, &vertices, &indices, Error::InvalidAsset)?;
    if bounds != calculate_bounds(&vertices) {
        return Err(Error::InvalidAsset(
            "serialized bounds do not match vertex positions".to_owned(),
        ));
    }
    Ok(MeshLod {
        geometric_error,
        bounds,
        vertices,
        indices,
    })
}

fn validate_primitive(
    attributes: VertexAttributes,
    lods: &[MeshLod],
    error: fn(String) -> Error,
) -> Result<(), Error> {
    if !attributes.contains(VertexAttributes::POSITION) {
        return Err(error("primitive is missing positions".to_owned()));
    }
    if lods.is_empty() || lods.len() > MAX_LODS {
        return Err(error(format!(
            "primitive must contain from 1 through {MAX_LODS} LODs"
        )));
    }
    let mut previous_indices = usize::MAX;
    let mut previous_error = 0.0_f32;
    for lod in lods {
        validate_lod(lod.geometric_error, &lod.vertices, &lod.indices, error)?;
        if lod.indices.len() >= previous_indices {
            return Err(error(
                "LOD index counts must be strictly decreasing".to_owned(),
            ));
        }
        if lod.geometric_error < previous_error {
            return Err(error(
                "LOD geometric errors must be non-decreasing".to_owned(),
            ));
        }
        previous_indices = lod.indices.len();
        previous_error = lod.geometric_error;
    }
    Ok(())
}

fn validate_lod(
    geometric_error: f32,
    vertices: &[MeshVertex],
    indices: &[u32],
    error: fn(String) -> Error,
) -> Result<(), Error> {
    if !geometric_error.is_finite() || geometric_error < 0.0 {
        return Err(error(
            "LOD geometric error must be finite and non-negative".to_owned(),
        ));
    }
    if vertices.is_empty() || indices.is_empty() || !indices.len().is_multiple_of(3) {
        return Err(error(
            "LOD must contain vertices and triangle-list indices".to_owned(),
        ));
    }
    if vertices.iter().any(|vertex| !vertex_is_finite(vertex)) {
        return Err(error("LOD contains a non-finite vertex".to_owned()));
    }
    if indices
        .iter()
        .any(|&index| usize::try_from(index).map_or(true, |value| value >= vertices.len()))
    {
        return Err(error("LOD index is outside its vertex buffer".to_owned()));
    }
    Ok(())
}

fn vertex_is_finite(vertex: &MeshVertex) -> bool {
    vertex
        .position
        .iter()
        .chain(&vertex.normal)
        .chain(&vertex.tangent)
        .chain(&vertex.texcoord_0)
        .all(|value| value.is_finite())
}

fn calculate_bounds(vertices: &[MeshVertex]) -> Bounds {
    let mut min = vertices[0].position;
    let mut max = min;
    for vertex in &vertices[1..] {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex.position[axis]);
            max[axis] = max[axis].max(vertex.position[axis]);
        }
    }
    Bounds { min, max }
}

fn put_len(output: &mut BytesMut, value: usize, label: &str) -> Result<(), Error> {
    let value = u32::try_from(value)
        .map_err(|_error| Error::InvalidInput(format!("{label} count does not fit u32")))?;
    output.put_u32_le(value);
    Ok(())
}

fn put_floats<const N: usize>(output: &mut BytesMut, values: &[f32; N]) {
    for value in values {
        output.put_u32_le(value.to_bits());
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self
            .offset
            .checked_add(count)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| Error::InvalidAsset("unexpected end of mesh data".to_owned()))?;
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let bytes: [u8; 4] = self
            .take(size_of::<u32>())?
            .try_into()
            .map_err(|_error| Error::InvalidAsset("invalid u32 field".to_owned()))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn count(&mut self, label: &str, maximum: usize) -> Result<usize, Error> {
        let value = usize::try_from(self.u32()?)
            .map_err(|_error| Error::InvalidAsset(format!("{label} count does not fit usize")))?;
        if value > maximum {
            return Err(Error::InvalidAsset(format!(
                "{label} count exceeds {maximum}"
            )));
        }
        Ok(value)
    }

    fn count_for_bytes(&mut self, label: &str, stride: usize) -> Result<usize, Error> {
        let value = usize::try_from(self.u32()?)
            .map_err(|_error| Error::InvalidAsset(format!("{label} count does not fit usize")))?;
        let bytes = value
            .checked_mul(stride)
            .ok_or_else(|| Error::InvalidAsset(format!("{label} byte length overflow")))?;
        if bytes > self.remaining() {
            return Err(Error::InvalidAsset(format!(
                "{label} data exceeds the remaining mesh bytes"
            )));
        }
        Ok(value)
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
#[path = "../tests/unit/mesh.rs"]
mod tests;
