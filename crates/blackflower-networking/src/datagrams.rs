use std::num::NonZeroU64;

use crate::codec::{Reader, Writer};
use crate::wire::{MAX_BOOTSTRAP_BYTES, WireError, enforce_size};
use crate::{
    BootstrapId, CommandId, InputSequence, ProjectionDigest, ProtocolRevision, SimulationTick,
};

/// Maximum canonical control-frame payload carried by one input datagram.
pub const MAX_CONTROL_FRAME_BYTES: usize = 256;
/// Maximum complete input-flow payload inside the minimum useful DATAGRAM.
pub const MAX_INPUT_DATAGRAM_PAYLOAD_BYTES: usize = 1_000;
/// Maximum opaque command payload registered by a gameplay codec.
pub const MAX_COMMAND_BYTES: usize = 128;
/// Maximum number of redundant control frames in one input datagram.
pub const MAX_CONTROL_FRAMES: usize = 3;
/// Maximum number of discrete commands in one input datagram.
pub const MAX_COMMANDS: usize = 8;
/// Maximum snapshot chunks emitted for one authoritative tick.
pub const MAX_SNAPSHOT_CHUNKS: usize = 4;
/// Fixed v1 state-bootstrap header size.
pub const STATE_BOOTSTRAP_HEADER_BYTES: usize = 60;

/// Four-timestamp monotonic clock exchange carried by the time-sync flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeSyncMessage {
    /// Client request containing its transmit timestamp.
    Request {
        /// Correlation identity selected by the requester.
        exchange_id: u32,
        /// Client monotonic transmit time in microseconds.
        client_send_micros: u64,
    },
    /// Server response containing the complete timestamps known at send time.
    Response {
        /// Correlation identity copied from the request.
        exchange_id: u32,
        /// Client monotonic transmit time copied from the request.
        client_send_micros: u64,
        /// Server monotonic receive time.
        server_receive_micros: u64,
        /// Server monotonic transmit time.
        server_send_micros: u64,
    },
}

/// Encode one exact time-sync payload.
#[must_use]
pub fn encode_time_sync(message: TimeSyncMessage) -> Vec<u8> {
    let mut writer = Writer::with_capacity(29);
    match message {
        TimeSyncMessage::Request {
            exchange_id,
            client_send_micros,
        } => {
            writer.u8(1);
            writer.u32(exchange_id);
            writer.u64(client_send_micros);
        }
        TimeSyncMessage::Response {
            exchange_id,
            client_send_micros,
            server_receive_micros,
            server_send_micros,
        } => {
            writer.u8(2);
            writer.u32(exchange_id);
            writer.u64(client_send_micros);
            writer.u64(server_receive_micros);
            writer.u64(server_send_micros);
        }
    }
    writer.finish()
}

/// Decode one exact time-sync payload.
pub fn decode_time_sync(bytes: &[u8]) -> Result<TimeSyncMessage, WireError> {
    let mut reader = Reader::new(bytes);
    let message = match reader.u8()? {
        1 => TimeSyncMessage::Request {
            exchange_id: reader.u32()?,
            client_send_micros: reader.u64()?,
        },
        2 => TimeSyncMessage::Response {
            exchange_id: reader.u32()?,
            client_send_micros: reader.u64()?,
            server_receive_micros: reader.u64()?,
            server_send_micros: reader.u64()?,
        },
        value => return Err(WireError::UnknownMessage(value)),
    };
    reader.finish()?;
    Ok(message)
}

/// Timing and historical execution policy of a registered discrete command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandTimingClass {
    /// Continuous movement or stance transition, accepted eight ticks late.
    ContinuousMovement = 1,
    /// Jump-like edge, accepted eight ticks late.
    Jump = 2,
    /// World interaction, accepted twelve ticks late.
    Interaction = 3,
    /// Reload, equip, or use, accepted twenty-four ticks late.
    Inventory = 4,
    /// Hitscan query against read-only history, accepted thirty-two ticks late.
    RewindRay = 5,
    /// Projectile catch-up against read-only history, accepted sixteen ticks late.
    CatchUpBallistic = 6,
    /// Explosives and dynamic physics that may execute only on the current tick.
    CurrentTickOnly = 7,
}

impl TryFrom<u8> for CommandTimingClass {
    type Error = WireError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::ContinuousMovement),
            2 => Ok(Self::Jump),
            3 => Ok(Self::Interaction),
            4 => Ok(Self::Inventory),
            5 => Ok(Self::RewindRay),
            6 => Ok(Self::CatchUpBallistic),
            7 => Ok(Self::CurrentTickOnly),
            _ => Err(WireError::InvalidValue("command timing class")),
        }
    }
}

/// One canonical control frame produced at the 60 Hz control rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFrame {
    /// Monotonic frame identity.
    pub sequence: InputSequence,
    /// First authoritative tick covered by the four-tick frame.
    pub execute_tick: SimulationTick,
    /// Opaque canonical bytes owned by the registered control codec.
    pub payload: Vec<u8>,
}

/// One idempotent discrete command carried redundantly with control frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscreteCommand {
    /// Stable idempotency identity.
    pub command_id: CommandId,
    /// Control frame from which the command originated.
    pub origin_input_sequence: InputSequence,
    /// Requested authoritative execution tick.
    pub execute_tick: SimulationTick,
    /// Historical view tick when the timing class permits rewind.
    pub view_tick: Option<SimulationTick>,
    /// Network-level timing policy.
    pub timing_class: CommandTimingClass,
    /// Kind interpreted by the revision-specific gameplay command codec.
    pub kind: u16,
    /// Opaque canonical bytes owned by that codec.
    pub payload: Vec<u8>,
}

/// Client-to-server input flow payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDatagram {
    /// Generation of the controlled-object binding.
    pub control_epoch: u32,
    /// Non-zero replicated identity controlled by this input.
    pub controlled_entity: NonZeroU64,
    /// Current control frame followed by up to two exact predecessors.
    pub frames: Vec<ControlFrame>,
    /// Idempotent discrete commands, including redundant retransmission.
    pub commands: Vec<DiscreteCommand>,
    /// Optional piggybacked application-level snapshot acknowledgement.
    pub applied_snapshot: Option<SnapshotAppliedAck>,
}

/// Encode one bounded input-flow payload.
pub fn encode_input_datagram(input: &InputDatagram) -> Result<Vec<u8>, WireError> {
    validate_input_counts(input)?;
    validate_frame_redundancy(&input.frames)?;
    let mut writer = Writer::with_capacity(64);
    writer.u32(input.control_epoch);
    writer.u64(input.controlled_entity.get());
    writer.u8(count_u8(input.frames.len())?);
    writer.u8(count_u8(input.commands.len())?);
    writer.u8(u8::from(input.applied_snapshot.is_some()));
    writer.u8(0);
    for frame in &input.frames {
        encode_control_frame(&mut writer, frame)?;
    }
    for command in &input.commands {
        encode_command(&mut writer, command)?;
    }
    if let Some(ack) = input.applied_snapshot {
        encode_ack_to(&mut writer, ack);
    }
    let bytes = writer.finish();
    enforce_size(bytes.len(), MAX_INPUT_DATAGRAM_PAYLOAD_BYTES)?;
    Ok(bytes)
}

/// Decode one exact bounded input-flow payload.
pub fn decode_input_datagram(bytes: &[u8]) -> Result<InputDatagram, WireError> {
    enforce_size(bytes.len(), MAX_INPUT_DATAGRAM_PAYLOAD_BYTES)?;
    let mut reader = Reader::new(bytes);
    let control_epoch = reader.u32()?;
    let controlled_entity = NonZeroU64::new(reader.u64()?)
        .ok_or(WireError::InvalidValue("controlled entity is zero"))?;
    let frame_count = bounded_count(reader.u8()?, 1, MAX_CONTROL_FRAMES)?;
    let command_count = bounded_count(reader.u8()?, 0, MAX_COMMANDS)?;
    let has_ack = read_bool(reader.u8()?)?;
    if reader.u8()? != 0 {
        return Err(WireError::Reserved);
    }
    let frames = decode_frames(&mut reader, frame_count)?;
    validate_frame_redundancy(&frames)?;
    let commands = decode_commands(&mut reader, command_count)?;
    let applied_snapshot = has_ack.then(|| decode_ack_from(&mut reader)).transpose()?;
    reader.finish()?;
    Ok(InputDatagram {
        control_epoch,
        controlled_entity,
        frames,
        commands,
        applied_snapshot,
    })
}

/// One all-or-nothing chunk of an authoritative snapshot delta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotChunk {
    /// Authoritative tick represented by the reconstructed projection.
    pub snapshot_tick: SimulationTick,
    /// Exact applied baseline tick, or no baseline for a full projection.
    pub baseline_tick: Option<SimulationTick>,
    /// Digest expected after all chunks are applied.
    pub projection_digest: ProjectionDigest,
    /// Zero-based chunk position.
    pub chunk_index: u8,
    /// Total chunks for this tick, from one through four.
    pub chunk_count: u8,
    /// Fragment bytes interpreted by replication.
    pub payload: Vec<u8>,
}

/// Encode one snapshot chunk within the negotiated payload limit.
pub fn encode_snapshot_chunk(
    chunk: &SnapshotChunk,
    maximum_payload: usize,
) -> Result<Vec<u8>, WireError> {
    validate_chunk_position(chunk.chunk_index, chunk.chunk_count)?;
    enforce_size(chunk.payload.len(), maximum_payload)?;
    let mut writer = Writer::with_capacity(52 + chunk.payload.len());
    writer.u64(chunk.snapshot_tick.get());
    writer.u8(u8::from(chunk.baseline_tick.is_some()));
    writer.u8(chunk.chunk_index);
    writer.u8(chunk.chunk_count);
    writer.u8(0);
    writer.u64(chunk.baseline_tick.map_or(0, SimulationTick::get));
    writer.fixed(chunk.projection_digest.as_bytes());
    writer.fixed(&chunk.payload);
    Ok(writer.finish())
}

/// Decode one exact snapshot chunk within the negotiated payload limit.
pub fn decode_snapshot_chunk(
    bytes: &[u8],
    maximum_payload: usize,
) -> Result<SnapshotChunk, WireError> {
    let mut reader = Reader::new(bytes);
    let snapshot_tick = SimulationTick::new(reader.u64()?);
    let has_baseline = read_bool(reader.u8()?)?;
    let chunk_index = reader.u8()?;
    let chunk_count = reader.u8()?;
    validate_chunk_position(chunk_index, chunk_count)?;
    if reader.u8()? != 0 {
        return Err(WireError::Reserved);
    }
    let baseline_value = reader.u64()?;
    if !has_baseline && baseline_value != 0 {
        return Err(WireError::Reserved);
    }
    let projection_digest = ProjectionDigest::from_bytes(reader.fixed()?);
    let payload = reader.remainder(maximum_payload)?.to_vec();
    reader.finish()?;
    Ok(SnapshotChunk {
        snapshot_tick,
        baseline_tick: has_baseline.then(|| SimulationTick::new(baseline_value)),
        projection_digest,
        chunk_index,
        chunk_count,
        payload,
    })
}

/// Application-level confirmation of one reconstructed and applied snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotAppliedAck {
    /// Applied authoritative tick.
    pub snapshot_tick: SimulationTick,
    /// Digest of the actual reconstructed canonical projection.
    pub projection_digest: ProjectionDigest,
}

/// Encode one exact applied-snapshot acknowledgement.
#[must_use]
pub fn encode_snapshot_applied_ack(ack: SnapshotAppliedAck) -> Vec<u8> {
    let mut writer = Writer::with_capacity(40);
    encode_ack_to(&mut writer, ack);
    writer.finish()
}

/// Decode one exact applied-snapshot acknowledgement.
pub fn decode_snapshot_applied_ack(bytes: &[u8]) -> Result<SnapshotAppliedAck, WireError> {
    let mut reader = Reader::new(bytes);
    let ack = decode_ack_from(&mut reader)?;
    reader.finish()?;
    Ok(ack)
}

/// Header preceding one uncompressed full-state bootstrap body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateBootstrapHeader {
    /// Transfer identity announced on session control.
    pub bootstrap_id: BootstrapId,
    /// Exact protocol revision used for the body.
    pub protocol_revision: ProtocolRevision,
    /// Full-state authoritative tick.
    pub snapshot_tick: SimulationTick,
    /// Expected digest after reconstructing the canonical projection.
    pub projection_digest: ProjectionDigest,
    /// Exact uncompressed body length.
    pub body_length: u32,
}

/// Encode one fixed state-bootstrap header.
pub fn encode_state_bootstrap_header(
    header: StateBootstrapHeader,
) -> Result<[u8; STATE_BOOTSTRAP_HEADER_BYTES], WireError> {
    let length =
        usize::try_from(header.body_length).map_err(|_error| WireError::IntegerOutOfRange)?;
    enforce_size(length, MAX_BOOTSTRAP_BYTES)?;
    let mut writer = Writer::with_capacity(STATE_BOOTSTRAP_HEADER_BYTES);
    writer.u64(header.bootstrap_id.get());
    writer.u32(header.protocol_revision.get());
    writer.u64(header.snapshot_tick.get());
    writer.fixed(header.projection_digest.as_bytes());
    writer.u32(header.body_length);
    writer.u32(0);
    writer
        .finish()
        .try_into()
        .map_err(|_bytes: Vec<u8>| WireError::InvalidValue("bootstrap header length"))
}

/// Decode and validate one fixed state-bootstrap header.
pub fn decode_state_bootstrap_header(bytes: &[u8]) -> Result<StateBootstrapHeader, WireError> {
    if bytes.len() != STATE_BOOTSTRAP_HEADER_BYTES {
        return Err(if bytes.len() < STATE_BOOTSTRAP_HEADER_BYTES {
            WireError::Truncated
        } else {
            WireError::Trailing
        });
    }
    let mut reader = Reader::new(bytes);
    let header = StateBootstrapHeader {
        bootstrap_id: BootstrapId::new(reader.u64()?),
        protocol_revision: ProtocolRevision::new(reader.u32()?),
        snapshot_tick: SimulationTick::new(reader.u64()?),
        projection_digest: ProjectionDigest::from_bytes(reader.fixed()?),
        body_length: reader.u32()?,
    };
    if reader.u32()? != 0 {
        return Err(WireError::Reserved);
    }
    reader.finish()?;
    let length =
        usize::try_from(header.body_length).map_err(|_error| WireError::IntegerOutOfRange)?;
    enforce_size(length, MAX_BOOTSTRAP_BYTES)?;
    Ok(header)
}

fn validate_input_counts(input: &InputDatagram) -> Result<(), WireError> {
    bounded_count(count_u8(input.frames.len())?, 1, MAX_CONTROL_FRAMES)?;
    bounded_count(count_u8(input.commands.len())?, 0, MAX_COMMANDS)?;
    Ok(())
}

fn validate_frame_redundancy(frames: &[ControlFrame]) -> Result<(), WireError> {
    let current = frames.first().ok_or(WireError::InvalidValue(
        "input datagram has no current frame",
    ))?;
    for (age, frame) in frames.iter().enumerate() {
        let age = u64::try_from(age).map_err(|_error| WireError::IntegerOutOfRange)?;
        let expected_sequence = current
            .sequence
            .get()
            .checked_sub(age)
            .ok_or(WireError::InvalidValue("input frame redundancy"))?;
        let expected_tick = current
            .execute_tick
            .get()
            .checked_sub(age.saturating_mul(4))
            .ok_or(WireError::InvalidValue("input frame redundancy"))?;
        if frame.sequence.get() != expected_sequence || frame.execute_tick.get() != expected_tick {
            return Err(WireError::InvalidValue("input frame redundancy"));
        }
    }
    Ok(())
}

fn encode_control_frame(writer: &mut Writer, frame: &ControlFrame) -> Result<(), WireError> {
    enforce_size(frame.payload.len(), MAX_CONTROL_FRAME_BYTES)?;
    writer.u64(frame.sequence.get());
    writer.u64(frame.execute_tick.get());
    writer.bytes_u16(&frame.payload)
}

fn encode_command(writer: &mut Writer, command: &DiscreteCommand) -> Result<(), WireError> {
    enforce_size(command.payload.len(), MAX_COMMAND_BYTES)?;
    writer.u64(command.command_id.get());
    writer.u64(command.origin_input_sequence.get());
    writer.u64(command.execute_tick.get());
    writer.u64(command.view_tick.map_or(0, SimulationTick::get));
    writer.u8(u8::from(command.view_tick.is_some()));
    writer.u8(command.timing_class as u8);
    writer.u16(command.kind);
    writer.bytes_u16(&command.payload)
}

fn decode_frames(reader: &mut Reader<'_>, count: usize) -> Result<Vec<ControlFrame>, WireError> {
    let mut frames = Vec::with_capacity(count);
    for _index in 0..count {
        frames.push(ControlFrame {
            sequence: InputSequence::new(reader.u64()?),
            execute_tick: SimulationTick::new(reader.u64()?),
            payload: reader.bytes_u16(MAX_CONTROL_FRAME_BYTES)?,
        });
    }
    Ok(frames)
}

fn decode_commands(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<DiscreteCommand>, WireError> {
    let mut commands = Vec::with_capacity(count);
    for _index in 0..count {
        commands.push(decode_command(reader)?);
    }
    Ok(commands)
}

fn decode_command(reader: &mut Reader<'_>) -> Result<DiscreteCommand, WireError> {
    let command_id = CommandId::new(reader.u64()?);
    let origin_input_sequence = InputSequence::new(reader.u64()?);
    let execute_tick = SimulationTick::new(reader.u64()?);
    let view_tick_value = reader.u64()?;
    let has_view_tick = read_bool(reader.u8()?)?;
    if !has_view_tick && view_tick_value != 0 {
        return Err(WireError::Reserved);
    }
    Ok(DiscreteCommand {
        command_id,
        origin_input_sequence,
        execute_tick,
        view_tick: has_view_tick.then(|| SimulationTick::new(view_tick_value)),
        timing_class: CommandTimingClass::try_from(reader.u8()?)?,
        kind: reader.u16()?,
        payload: reader.bytes_u16(MAX_COMMAND_BYTES)?,
    })
}

fn encode_ack_to(writer: &mut Writer, ack: SnapshotAppliedAck) {
    writer.u64(ack.snapshot_tick.get());
    writer.fixed(ack.projection_digest.as_bytes());
}

fn decode_ack_from(reader: &mut Reader<'_>) -> Result<SnapshotAppliedAck, WireError> {
    Ok(SnapshotAppliedAck {
        snapshot_tick: SimulationTick::new(reader.u64()?),
        projection_digest: ProjectionDigest::from_bytes(reader.fixed()?),
    })
}

fn validate_chunk_position(index: u8, count: u8) -> Result<(), WireError> {
    if count == 0 || usize::from(count) > MAX_SNAPSHOT_CHUNKS || index >= count {
        Err(WireError::InvalidValue("snapshot chunk position"))
    } else {
        Ok(())
    }
}

fn count_u8(value: usize) -> Result<u8, WireError> {
    u8::try_from(value).map_err(|_error| WireError::IntegerOutOfRange)
}

fn bounded_count(value: u8, minimum: usize, maximum: usize) -> Result<usize, WireError> {
    let actual = usize::from(value);
    if actual < minimum || actual > maximum {
        Err(WireError::ExcessiveCount { actual, maximum })
    } else {
        Ok(actual)
    }
}

fn read_bool(value: u8) -> Result<bool, WireError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(WireError::InvalidValue("wire boolean")),
    }
}
