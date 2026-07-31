use std::collections::BTreeSet;

use glam::Vec3A;

use crate::{Error, STEAM_AUDIO_VERSION};

/// Schema shared by `.bfacscn`, `.bfacprb`, and `.bfac`.
pub const ACOUSTIC_ASSET_SCHEMA: u32 = 1;

const SCENE_MAGIC: &[u8; 8] = b"BFACSCN\0";
const PROBES_MAGIC: &[u8; 8] = b"BFACPRB\0";
const ENVIRONMENT_MAGIC: &[u8; 8] = b"BFAC\0\0\0\0";
const CHECKSUM_BYTES: usize = 32;
const MAX_TEXT_BYTES: usize = 255;

/// Serialized immutable Steam Audio scene stored in `.bfacscn`.
#[derive(Debug, Clone)]
pub struct AcousticScene {
    bytes: Vec<u8>,
    serialized: Vec<u8>,
    vertex_count: u32,
    triangle_count: u32,
    material_count: u32,
}

impl AcousticScene {
    /// Decode and validate one complete `.bfacscn` object.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(bytes, "acoustic scene");
        reader.magic(SCENE_MAGIC)?;
        reader.schema()?;
        reader.sdk_version()?;
        let vertex_count = reader.u32()?;
        let triangle_count = reader.u32()?;
        let material_count = reader.u32()?;
        if vertex_count == 0 || triangle_count == 0 || material_count == 0 {
            return Err(invalid(
                "acoustic scene",
                "geometry counts must be non-zero",
            ));
        }
        let serialized = reader.hashed_payload()?.to_vec();
        reader.finish()?;
        Ok(Self {
            bytes: bytes.to_vec(),
            serialized,
            vertex_count,
            triangle_count,
            material_count,
        })
    }

    pub(crate) fn encode(
        serialized: Vec<u8>,
        vertex_count: u32,
        triangle_count: u32,
        material_count: u32,
    ) -> Result<Self, Error> {
        if serialized.is_empty() || vertex_count == 0 || triangle_count == 0 || material_count == 0
        {
            return Err(invalid("acoustic scene", "scene payload is empty"));
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(SCENE_MAGIC);
        push_common_header(&mut bytes);
        push_u32(&mut bytes, vertex_count);
        push_u32(&mut bytes, triangle_count);
        push_u32(&mut bytes, material_count);
        push_hashed_payload(&mut bytes, &serialized)?;
        Ok(Self {
            bytes,
            serialized,
            vertex_count,
            triangle_count,
            material_count,
        })
    }

    /// Complete cooked object bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Number of transformed vertices in the serialized scene.
    #[must_use]
    pub const fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    /// Number of triangles in the serialized scene.
    #[must_use]
    pub const fn triangle_count(&self) -> u32 {
        self.triangle_count
    }

    /// Number of material table entries in the serialized scene.
    #[must_use]
    pub const fn material_count(&self) -> u32 {
        self.material_count
    }

    pub(crate) fn serialized(&self) -> &[u8] {
        &self.serialized
    }
}

/// One generated Steam Audio probe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcousticProbe {
    position: Vec3A,
    radius: f32,
}

impl AcousticProbe {
    pub(crate) fn new(position: Vec3A, radius: f32) -> Result<Self, Error> {
        if !position.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return Err(invalid("acoustic probes", "probe sphere is invalid"));
        }
        Ok(Self { position, radius })
    }

    /// World-space probe center.
    #[must_use]
    pub const fn position(self) -> Vec3A {
        self.position
    }

    /// Influence radius in meters.
    #[must_use]
    pub const fn radius(self) -> f32 {
        self.radius
    }
}

/// Steam Audio baked payload category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BakedDataType {
    /// Convolution reflections plus parametric reverb.
    Reflections,
    /// Probe-to-probe propagation graph.
    Pathing,
}

/// How source and listener positions vary for one baked layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BakedDataVariation {
    /// Source and listener are colocated at each probe.
    Reverb,
    /// Source is fixed and listener varies.
    StaticSource,
    /// Listener is fixed and source varies.
    StaticListener,
    /// Source and listener both vary.
    Dynamic,
}

/// Sphere used by Steam Audio to identify endpoint-specific baked data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BakedDataIdentifier {
    data_type: BakedDataType,
    variation: BakedDataVariation,
    endpoint: AcousticProbe,
}

impl BakedDataIdentifier {
    /// Base reflections and parametric reverb layer.
    pub fn reverb() -> Result<Self, Error> {
        Ok(Self {
            data_type: BakedDataType::Reflections,
            variation: BakedDataVariation::Reverb,
            endpoint: AcousticProbe::new(Vec3A::ZERO, f32::MIN_POSITIVE)?,
        })
    }

    /// Dynamic probe-to-probe pathing layer.
    pub fn dynamic_pathing() -> Result<Self, Error> {
        Ok(Self {
            data_type: BakedDataType::Pathing,
            variation: BakedDataVariation::Dynamic,
            endpoint: AcousticProbe::new(Vec3A::ZERO, f32::MIN_POSITIVE)?,
        })
    }

    /// Baked data category.
    #[must_use]
    pub const fn data_type(self) -> BakedDataType {
        self.data_type
    }

    /// Position-variation policy.
    #[must_use]
    pub const fn variation(self) -> BakedDataVariation {
        self.variation
    }

    /// Endpoint influence used for identifier equality.
    #[must_use]
    pub const fn endpoint(self) -> AcousticProbe {
        self.endpoint
    }
}

/// Size and identifier of one layer inside a probe batch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BakedLayer {
    identifier: BakedDataIdentifier,
    byte_len: u64,
}

impl BakedLayer {
    pub(crate) const fn new(identifier: BakedDataIdentifier, byte_len: u64) -> Self {
        Self {
            identifier,
            byte_len,
        }
    }

    /// Typed Steam Audio identifier.
    #[must_use]
    pub const fn identifier(self) -> BakedDataIdentifier {
        self.identifier
    }

    /// Native serialized size reported for the layer.
    #[must_use]
    pub const fn byte_len(self) -> u64 {
        self.byte_len
    }
}

/// Generated probes and baked layers stored in `.bfacprb`.
#[derive(Debug, Clone)]
pub struct ProbeBatch {
    bytes: Vec<u8>,
    serialized: Vec<u8>,
    zone: String,
    probes: Vec<AcousticProbe>,
    layers: Vec<BakedLayer>,
}

impl ProbeBatch {
    /// Decode and validate one complete `.bfacprb` object.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(bytes, "acoustic probes");
        reader.magic(PROBES_MAGIC)?;
        reader.schema()?;
        reader.sdk_version()?;
        let zone = reader.text()?.to_owned();
        let probe_count = reader.count()?;
        let layer_count = reader.count()?;
        let mut probes = Vec::new();
        for _ in 0..probe_count {
            probes.push(AcousticProbe::new(reader.vec3()?, reader.f32()?)?);
        }
        let mut layers = Vec::new();
        let mut identifiers = BTreeSet::new();
        for _ in 0..layer_count {
            let identifier = reader.identifier()?;
            if !identifiers.insert((identifier.data_type, identifier.variation)) {
                return Err(invalid("acoustic probes", "duplicate baked layer"));
            }
            let byte_len = reader.u64()?;
            if byte_len == 0 {
                return Err(invalid("acoustic probes", "baked layer is empty"));
            }
            layers.push(BakedLayer::new(identifier, byte_len));
        }
        if probes.is_empty() || layers.is_empty() {
            return Err(invalid(
                "acoustic probes",
                "probes and layers must be non-empty",
            ));
        }
        let serialized = reader.hashed_payload()?.to_vec();
        reader.finish()?;
        Ok(Self {
            bytes: bytes.to_vec(),
            serialized,
            zone,
            probes,
            layers,
        })
    }

    pub(crate) fn encode(
        zone: String,
        probes: Vec<AcousticProbe>,
        layers: Vec<BakedLayer>,
        serialized: Vec<u8>,
    ) -> Result<Self, Error> {
        validate_text(&zone, "acoustic probes")?;
        if probes.is_empty() || layers.is_empty() || serialized.is_empty() {
            return Err(invalid("acoustic probes", "batch content is empty"));
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(PROBES_MAGIC);
        push_common_header(&mut bytes);
        push_text(&mut bytes, &zone)?;
        push_len(&mut bytes, probes.len())?;
        push_len(&mut bytes, layers.len())?;
        for probe in &probes {
            push_vec3(&mut bytes, probe.position);
            push_f32(&mut bytes, probe.radius);
        }
        for layer in &layers {
            bytes.push(encode_data_type(layer.identifier.data_type));
            bytes.push(encode_variation(layer.identifier.variation));
            bytes.extend_from_slice(&[0, 0]);
            push_vec3(&mut bytes, layer.identifier.endpoint.position);
            push_f32(&mut bytes, layer.identifier.endpoint.radius);
            push_u64(&mut bytes, layer.byte_len);
        }
        push_hashed_payload(&mut bytes, &serialized)?;
        Ok(Self {
            bytes,
            serialized,
            zone,
            probes,
            layers,
        })
    }

    /// Complete cooked object bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Acoustic zone populated by this batch.
    #[must_use]
    pub fn zone(&self) -> &str {
        &self.zone
    }

    /// Deterministic Steam Audio probe order.
    #[must_use]
    pub fn probes(&self) -> &[AcousticProbe] {
        &self.probes
    }

    /// Baked layers stored in the native batch.
    #[must_use]
    pub fn layers(&self) -> &[BakedLayer] {
        &self.layers
    }

    pub(crate) fn serialized(&self) -> &[u8] {
        &self.serialized
    }
}

/// One zone entry in a `.bfac` acoustic environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcousticZone {
    id: String,
    scene: String,
    probes: String,
}

impl AcousticZone {
    /// Create a zone referencing one scene and one probe-batch asset ID.
    pub fn new(
        id: impl Into<String>,
        scene: impl Into<String>,
        probes: impl Into<String>,
    ) -> Result<Self, Error> {
        let value = Self {
            id: id.into(),
            scene: scene.into(),
            probes: probes.into(),
        };
        for text in [&value.id, &value.scene, &value.probes] {
            validate_text(text, "acoustic environment")?;
        }
        Ok(value)
    }

    /// Stable zone identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Referenced `.bfacscn` asset ID.
    #[must_use]
    pub fn scene(&self) -> &str {
        &self.scene
    }

    /// Referenced `.bfacprb` asset ID.
    #[must_use]
    pub fn probes(&self) -> &str {
        &self.probes
    }
}

/// Lightweight zone-to-scene/probes descriptor stored in `.bfac`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcousticEnvironment {
    bytes: Vec<u8>,
    zones: Vec<AcousticZone>,
}

impl AcousticEnvironment {
    /// Build a canonical descriptor ordered by zone ID.
    pub fn new(mut zones: Vec<AcousticZone>) -> Result<Self, Error> {
        if zones.is_empty() {
            return Err(invalid("acoustic environment", "zone list is empty"));
        }
        zones.sort_by(|left, right| left.id.cmp(&right.id));
        if zones.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(invalid("acoustic environment", "zone IDs are duplicated"));
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(ENVIRONMENT_MAGIC);
        push_u32(&mut bytes, ACOUSTIC_ASSET_SCHEMA);
        push_len(&mut bytes, zones.len())?;
        for zone in &zones {
            push_text(&mut bytes, &zone.id)?;
            push_text(&mut bytes, &zone.scene)?;
            push_text(&mut bytes, &zone.probes)?;
        }
        Ok(Self { bytes, zones })
    }

    /// Decode and validate one `.bfac` descriptor.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(bytes, "acoustic environment");
        reader.magic(ENVIRONMENT_MAGIC)?;
        reader.schema()?;
        let count = reader.count()?;
        let mut zones = Vec::new();
        for _ in 0..count {
            zones.push(AcousticZone::new(
                reader.text()?,
                reader.text()?,
                reader.text()?,
            )?);
        }
        reader.finish()?;
        let value = Self::new(zones)?;
        if value.bytes != bytes {
            return Err(invalid(
                "acoustic environment",
                "descriptor is not canonically ordered",
            ));
        }
        Ok(value)
    }

    /// Complete cooked object bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Canonically ordered zones.
    #[must_use]
    pub fn zones(&self) -> &[AcousticZone] {
        &self.zones
    }
}

fn push_common_header(output: &mut Vec<u8>) {
    push_u32(output, ACOUSTIC_ASSET_SCHEMA);
    let (major, minor, patch) = STEAM_AUDIO_VERSION;
    push_u32(output, (major << 16) | (minor << 8) | patch);
}

fn push_hashed_payload(output: &mut Vec<u8>, payload: &[u8]) -> Result<(), Error> {
    push_u64(
        output,
        u64::try_from(payload.len())
            .map_err(|_error| invalid("acoustic asset", "payload exceeds u64"))?,
    );
    output.extend_from_slice(blake3::hash(payload).as_bytes());
    output.extend_from_slice(payload);
    Ok(())
}

fn push_text(output: &mut Vec<u8>, value: &str) -> Result<(), Error> {
    validate_text(value, "acoustic asset")?;
    push_len(output, value.len())?;
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn push_len(output: &mut Vec<u8>, len: usize) -> Result<(), Error> {
    push_u32(
        output,
        u32::try_from(len).map_err(|_error| invalid("acoustic asset", "count exceeds u32"))?,
    );
    Ok(())
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_f32(output: &mut Vec<u8>, value: f32) {
    push_u32(output, value.to_bits());
}

fn push_vec3(output: &mut Vec<u8>, value: Vec3A) {
    push_f32(output, value.x);
    push_f32(output, value.y);
    push_f32(output, value.z);
}

fn validate_text(value: &str, format: &'static str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > MAX_TEXT_BYTES
        || !value.is_ascii()
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(invalid(format, "text field is invalid"))
    } else {
        Ok(())
    }
}

fn encode_data_type(value: BakedDataType) -> u8 {
    match value {
        BakedDataType::Reflections => 0,
        BakedDataType::Pathing => 1,
    }
}

fn encode_variation(value: BakedDataVariation) -> u8 {
    match value {
        BakedDataVariation::Reverb => 0,
        BakedDataVariation::StaticSource => 1,
        BakedDataVariation::StaticListener => 2,
        BakedDataVariation::Dynamic => 3,
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
    format: &'static str,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8], format: &'static str) -> Self {
        Self {
            bytes,
            offset: 0,
            format,
        }
    }

    fn magic(&mut self, expected: &[u8]) -> Result<(), Error> {
        if self.take(expected.len())? == expected {
            Ok(())
        } else {
            Err(invalid(self.format, "magic is invalid"))
        }
    }

    fn schema(&mut self) -> Result<(), Error> {
        if self.u32()? == ACOUSTIC_ASSET_SCHEMA {
            Ok(())
        } else {
            Err(invalid(self.format, "schema is unsupported"))
        }
    }

    fn sdk_version(&mut self) -> Result<(), Error> {
        let (major, minor, patch) = STEAM_AUDIO_VERSION;
        if self.u32()? == (major << 16) | (minor << 8) | patch {
            Ok(())
        } else {
            Err(invalid(self.format, "Steam Audio version is incompatible"))
        }
    }

    fn count(&mut self) -> Result<usize, Error> {
        usize::try_from(self.u32()?).map_err(|_error| invalid(self.format, "count exceeds usize"))
    }

    fn text(&mut self) -> Result<&'a str, Error> {
        let length = self.count()?;
        let value = std::str::from_utf8(self.take(length)?)
            .map_err(|_error| invalid(self.format, "text is not UTF-8"))?;
        validate_text(value, self.format)?;
        Ok(value)
    }

    fn identifier(&mut self) -> Result<BakedDataIdentifier, Error> {
        let data_type = match self.byte()? {
            0 => BakedDataType::Reflections,
            1 => BakedDataType::Pathing,
            _ => return Err(invalid(self.format, "baked data type is invalid")),
        };
        let variation = match self.byte()? {
            0 => BakedDataVariation::Reverb,
            1 => BakedDataVariation::StaticSource,
            2 => BakedDataVariation::StaticListener,
            3 => BakedDataVariation::Dynamic,
            _ => return Err(invalid(self.format, "baked variation is invalid")),
        };
        if self.take(2)? != [0, 0] {
            return Err(invalid(self.format, "reserved layer bytes are non-zero"));
        }
        let endpoint = AcousticProbe::new(self.vec3()?, self.f32()?)?;
        match (data_type, variation) {
            (BakedDataType::Reflections, BakedDataVariation::Reverb)
            | (BakedDataType::Pathing, BakedDataVariation::Dynamic) => {}
            _ => return Err(invalid(self.format, "unsupported Stage 8 baked layer")),
        }
        Ok(BakedDataIdentifier {
            data_type,
            variation,
            endpoint,
        })
    }

    fn vec3(&mut self) -> Result<Vec3A, Error> {
        Ok(Vec3A::new(self.f32()?, self.f32()?, self.f32()?))
    }

    fn f32(&mut self) -> Result<f32, Error> {
        let value = f32::from_bits(self.u32()?);
        if value.is_finite() {
            Ok(value)
        } else {
            Err(invalid(self.format, "float is non-finite"))
        }
    }

    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_error| invalid(self.format, "u32 is truncated"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, Error> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_error| invalid(self.format, "u64 is truncated"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn hashed_payload(&mut self) -> Result<&'a [u8], Error> {
        let length = usize::try_from(self.u64()?)
            .map_err(|_error| invalid(self.format, "payload length exceeds usize"))?;
        let checksum: [u8; CHECKSUM_BYTES] = self
            .take(CHECKSUM_BYTES)?
            .try_into()
            .map_err(|_error| invalid(self.format, "checksum is truncated"))?;
        let payload = self.take(length)?;
        if payload.is_empty() || blake3::hash(payload).as_bytes() != &checksum {
            return Err(invalid(self.format, "payload checksum does not match"));
        }
        Ok(payload)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], Error> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| invalid(self.format, "offset overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| invalid(self.format, "object is truncated"))?;
        self.offset = end;
        Ok(value)
    }

    fn finish(self) -> Result<(), Error> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(invalid(self.format, "object contains trailing bytes"))
        }
    }
}

const fn invalid(format: &'static str, reason: &'static str) -> Error {
    Error::InvalidAcousticAsset { format, reason }
}

#[cfg(test)]
mod tests {
    use super::{
        AcousticEnvironment, AcousticProbe, AcousticScene, AcousticZone, BakedDataIdentifier,
        BakedLayer, ProbeBatch,
    };
    use crate::Vec3A;

    #[test]
    fn scene_and_probe_formats_round_trip_and_reject_corruption() -> Result<(), crate::Error> {
        let scene = AcousticScene::encode(vec![1, 2, 3], 3, 1, 1)?;
        let decoded = AcousticScene::from_bytes(scene.bytes())?;
        assert_eq!(decoded.triangle_count(), 1);
        let mut corrupt = scene.bytes().to_vec();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        assert!(AcousticScene::from_bytes(&corrupt).is_err());

        let reverb = BakedDataIdentifier::reverb()?;
        let pathing = BakedDataIdentifier::dynamic_pathing()?;
        let batch = ProbeBatch::encode(
            "ground_floor".to_owned(),
            vec![AcousticProbe::new(Vec3A::Y, 2.0)?],
            vec![BakedLayer::new(reverb, 10), BakedLayer::new(pathing, 20)],
            vec![4, 5, 6],
        )?;
        let decoded = ProbeBatch::from_bytes(batch.bytes())?;
        assert_eq!(decoded.zone(), "ground_floor");
        assert_eq!(decoded.probes().len(), 1);
        assert_eq!(decoded.layers().len(), 2);
        Ok(())
    }

    #[test]
    fn environment_is_sorted_and_strict() -> Result<(), crate::Error> {
        let environment = AcousticEnvironment::new(vec![
            AcousticZone::new("upper", "levels/scene", "levels/upper")?,
            AcousticZone::new("ground", "levels/scene", "levels/ground")?,
        ])?;
        assert_eq!(environment.zones()[0].id(), "ground");
        assert_eq!(
            AcousticEnvironment::from_bytes(environment.bytes())?.zones(),
            environment.zones()
        );
        Ok(())
    }
}
