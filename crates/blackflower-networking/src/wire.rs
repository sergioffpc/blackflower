use crate::{ConnectionEpoch, FlowSequence, ProjectionDigest, ProtocolRevision, SimulationTick};

/// Version of the common Blackflower application datagram header.
pub const WIRE_VERSION: u8 = 1;
/// Bytes occupied by the common datagram header.
pub const DATAGRAM_HEADER_BYTES: usize = 11;
/// Minimum application payload required after the common header.
pub const MINIMUM_USEFUL_DATAGRAM_BYTES: usize = 1_000;
/// Minimum QUIC DATAGRAM size accepted for a v1 connection.
pub const MINIMUM_QUIC_DATAGRAM_BYTES: usize =
    DATAGRAM_HEADER_BYTES + MINIMUM_USEFUL_DATAGRAM_BYTES;
/// Maximum reliable session-control payload.
pub const MAX_CONTROL_MESSAGE_BYTES: usize = 16 * 1_024;
/// Desired upper bound for an ordinary bootstrap.
pub const TARGET_BOOTSTRAP_BYTES: usize = 512 * 1_024;
/// Absolute full-state bootstrap bound.
pub const MAX_BOOTSTRAP_BYTES: usize = 2 * 1_024 * 1_024;

/// Datagram flow carried by the common application header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum FlowId {
    /// Four-timestamp monotonic clock exchange.
    TimeSync = 1,
    /// Client control frames and discrete commands.
    Input = 2,
    /// One chunk of an authoritative snapshot delta.
    SnapshotDelta = 3,
    /// Application-level acknowledgement of an applied projection.
    SnapshotAppliedAck = 4,
    /// Opaque Opus capture payload from a client.
    VoiceCapture = 5,
    /// Authoritatively routed audible voice payload.
    VoiceDelivery = 6,
}

impl TryFrom<u8> for FlowId {
    type Error = WireError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::TimeSync),
            2 => Ok(Self::Input),
            3 => Ok(Self::SnapshotDelta),
            4 => Ok(Self::SnapshotAppliedAck),
            5 => Ok(Self::VoiceCapture),
            6 => Ok(Self::VoiceDelivery),
            value => Err(WireError::UnknownFlow(value)),
        }
    }
}

/// Fixed common header of every application datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatagramHeader {
    /// Flow-specific payload type.
    pub flow: FlowId,
    /// Connection generation that produced this datagram.
    pub connection_epoch: ConnectionEpoch,
    /// Monotonic sequence within `flow`.
    pub flow_sequence: FlowSequence,
}

/// Borrowed decoded application datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedDatagram<'a> {
    /// Validated common header.
    pub header: DatagramHeader,
    /// Flow-specific payload bytes.
    pub payload: &'a [u8],
}

/// Purpose declared at the start of a reliable stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StreamKind {
    /// Long-lived bidirectional session-control stream.
    SessionControl = 1,
    /// Server-to-client full-state bootstrap stream.
    StateBootstrap = 2,
}

impl TryFrom<u8> for StreamKind {
    type Error = WireError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::SessionControl),
            2 => Ok(Self::StateBootstrap),
            value => Err(WireError::UnknownStream(value)),
        }
    }
}

/// Invalid or non-canonical wire data.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    /// Input ended before the declared value was complete.
    #[error("wire value is truncated")]
    Truncated,
    /// Bytes remained after the declared value.
    #[error("wire value has trailing bytes")]
    Trailing,
    /// The common wire version is unsupported.
    #[error("unsupported wire version {0}")]
    UnsupportedVersion(u8),
    /// A reserved value was non-zero.
    #[error("reserved wire value is non-zero")]
    Reserved,
    /// The datagram flow is unknown.
    #[error("unknown datagram flow {0}")]
    UnknownFlow(u8),
    /// The stream purpose is unknown.
    #[error("unknown stream kind {0}")]
    UnknownStream(u8),
    /// A message kind is unknown for its stream.
    #[error("unknown message kind {0}")]
    UnknownMessage(u8),
    /// A payload exceeded a protocol bound.
    #[error("wire payload has {actual} bytes, maximum is {maximum}")]
    Oversized {
        /// Actual byte count.
        actual: usize,
        /// Protocol maximum.
        maximum: usize,
    },
    /// An integer cannot be represented by the requested wire type.
    #[error("wire integer is out of range")]
    IntegerOutOfRange,
    /// A declared collection count violates its schema.
    #[error("wire collection count {actual} exceeds maximum {maximum}")]
    ExcessiveCount {
        /// Declared item count.
        actual: usize,
        /// Protocol maximum.
        maximum: usize,
    },
    /// A field had a structurally invalid value.
    #[error("wire field is invalid: {0}")]
    InvalidValue(&'static str),
}

/// Encode one common application datagram.
pub fn encode_datagram(header: DatagramHeader, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(DATAGRAM_HEADER_BYTES + payload.len());
    bytes.push(WIRE_VERSION);
    bytes.push(header.flow as u8);
    bytes.extend_from_slice(&header.connection_epoch.get().to_le_bytes());
    bytes.extend_from_slice(&header.flow_sequence.get().to_le_bytes());
    bytes.push(0);
    bytes.extend_from_slice(payload);
    bytes
}

/// Decode and validate the common application datagram header.
pub fn decode_datagram(bytes: &[u8]) -> Result<DecodedDatagram<'_>, WireError> {
    if bytes.len() < DATAGRAM_HEADER_BYTES {
        return Err(WireError::Truncated);
    }
    if bytes[0] != WIRE_VERSION {
        return Err(WireError::UnsupportedVersion(bytes[0]));
    }
    if bytes[10] != 0 {
        return Err(WireError::Reserved);
    }
    let header = decode_datagram_header(bytes)?;
    Ok(DecodedDatagram {
        header,
        payload: &bytes[DATAGRAM_HEADER_BYTES..],
    })
}

fn decode_datagram_header(bytes: &[u8]) -> Result<DatagramHeader, WireError> {
    let epoch = u32::from_le_bytes(copy_array(&bytes[2..6])?);
    let sequence = u32::from_le_bytes(copy_array(&bytes[6..10])?);
    Ok(DatagramHeader {
        flow: FlowId::try_from(bytes[1])?,
        connection_epoch: ConnectionEpoch::new(epoch),
        flow_sequence: FlowSequence::new(sequence),
    })
}

/// Encode the four-byte preamble that identifies one reliable stream.
#[must_use]
pub fn encode_stream_preamble(kind: StreamKind) -> [u8; 4] {
    [WIRE_VERSION, kind as u8, 0, 0]
}

/// Decode a reliable-stream preamble.
pub fn decode_stream_preamble(bytes: &[u8]) -> Result<StreamKind, WireError> {
    if bytes.len() != 4 {
        return Err(if bytes.len() < 4 {
            WireError::Truncated
        } else {
            WireError::Trailing
        });
    }
    if bytes[0] != WIRE_VERSION {
        return Err(WireError::UnsupportedVersion(bytes[0]));
    }
    if bytes[2..4] != [0, 0] {
        return Err(WireError::Reserved);
    }
    StreamKind::try_from(bytes[1])
}

/// Encode a kind-tagged, length-delimited reliable message.
pub fn encode_frame(kind: u8, payload: &[u8], maximum: usize) -> Result<Vec<u8>, WireError> {
    enforce_size(payload.len(), maximum)?;
    let length = u64::try_from(payload.len()).map_err(|_error| WireError::IntegerOutOfRange)?;
    let mut bytes = Vec::with_capacity(1 + 8 + payload.len());
    bytes.push(kind);
    encode_varint(length, &mut bytes)?;
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

/// Decode exactly one kind-tagged reliable message.
pub fn decode_frame(bytes: &[u8], maximum: usize) -> Result<(u8, &[u8]), WireError> {
    let Some((&kind, remainder)) = bytes.split_first() else {
        return Err(WireError::Truncated);
    };
    let (length, length_bytes) = decode_varint(remainder)?;
    let length = usize::try_from(length).map_err(|_error| WireError::IntegerOutOfRange)?;
    enforce_size(length, maximum)?;
    let payload = remainder.get(length_bytes..).ok_or(WireError::Truncated)?;
    if payload.len() != length {
        return Err(if payload.len() < length {
            WireError::Truncated
        } else {
            WireError::Trailing
        });
    }
    Ok((kind, payload))
}

/// Compute the domain-separated digest used by applied-snapshot ACKs.
#[must_use]
pub fn projection_digest(
    revision: ProtocolRevision,
    tick: SimulationTick,
    canonical_projection: &[u8],
) -> ProjectionDigest {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"blackflower.snapshot.projection.v1\0");
    hasher.update(&revision.get().to_le_bytes());
    hasher.update(&tick.get().to_le_bytes());
    hasher.update(canonical_projection);
    ProjectionDigest::from_bytes(*hasher.finalize().as_bytes())
}

pub(crate) fn enforce_size(actual: usize, maximum: usize) -> Result<(), WireError> {
    if actual > maximum {
        Err(WireError::Oversized { actual, maximum })
    } else {
        Ok(())
    }
}

pub(crate) fn copy_array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], WireError> {
    <[u8; N]>::try_from(bytes).map_err(|_error| WireError::Truncated)
}

fn encode_varint(value: u64, output: &mut Vec<u8>) -> Result<(), WireError> {
    match value {
        0..=63 => output.push(u8::try_from(value).map_err(|_error| WireError::IntegerOutOfRange)?),
        64..=16_383 => encode_varint_two(value, output)?,
        16_384..=1_073_741_823 => encode_varint_four(value, output)?,
        1_073_741_824..=4_611_686_018_427_387_903 => encode_varint_eight(value, output),
        _ => return Err(WireError::IntegerOutOfRange),
    }
    Ok(())
}

fn encode_varint_two(value: u64, output: &mut Vec<u8>) -> Result<(), WireError> {
    let encoded = u16::try_from(value).map_err(|_error| WireError::IntegerOutOfRange)? | 0x4000;
    output.extend_from_slice(&encoded.to_be_bytes());
    Ok(())
}

fn encode_varint_four(value: u64, output: &mut Vec<u8>) -> Result<(), WireError> {
    let encoded =
        u32::try_from(value).map_err(|_error| WireError::IntegerOutOfRange)? | 0x8000_0000;
    output.extend_from_slice(&encoded.to_be_bytes());
    Ok(())
}

fn encode_varint_eight(value: u64, output: &mut Vec<u8>) {
    output.extend_from_slice(&(value | 0xc000_0000_0000_0000).to_be_bytes());
}

fn decode_varint(bytes: &[u8]) -> Result<(u64, usize), WireError> {
    let first = *bytes.first().ok_or(WireError::Truncated)?;
    let length = 1_usize << usize::from(first >> 6);
    let encoded = bytes.get(..length).ok_or(WireError::Truncated)?;
    let value = decode_varint_bytes(encoded);
    if length != canonical_varint_width(value) {
        return Err(WireError::InvalidValue("non-canonical QUIC varint"));
    }
    Ok((value, length))
}

const fn canonical_varint_width(value: u64) -> usize {
    match value {
        0..=63 => 1,
        64..=16_383 => 2,
        16_384..=1_073_741_823 => 4,
        _ => 8,
    }
}

fn decode_varint_bytes(bytes: &[u8]) -> u64 {
    let mut value = u64::from(bytes[0] & 0x3f);
    for &byte in &bytes[1..] {
        value = (value << 8) | u64::from(byte);
    }
    value
}
