use std::collections::BTreeMap;

use blackflower_acoustics::{
    AcousticStructureVersion, AudibleSoundDelivery, AudibleVoiceDelivery, BandEnergy, EncodedVoice,
    MAX_OPUS_PACKET_BYTES, PropagationDescriptor, VoiceStreamId,
};

/// Strict wire version for acoustic datagrams.
pub const ACOUSTIC_DATAGRAM_VERSION: u16 = 1;

const MAGIC: &[u8; 4] = b"BFAD";
const HEADER_BYTES: usize = 8;
/// Maximum accepted acoustic datagram size before production transport overhead.
pub const MAX_ACOUSTIC_DATAGRAM_BYTES: usize = 1_500;
const VOICE_CAPTURE: u8 = 1;
const AUDIBLE_SOUND: u8 = 2;
const AUDIBLE_VOICE: u8 = 3;
const PROPAGATION_BYTES: usize = 46;

/// Malformed, incompatible, or out-of-window acoustic datagram.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DatagramError {
    /// Header magic does not identify a Blackflower acoustic datagram.
    #[error("invalid acoustic datagram magic")]
    Magic,
    /// The protocol version is not supported.
    #[error("unsupported acoustic datagram version {0}")]
    Version(u16),
    /// The datagram kind differs from the requested decoder.
    #[error("unexpected acoustic datagram kind")]
    Kind,
    /// Reserved bytes are not zero.
    #[error("non-zero reserved acoustic datagram bits")]
    Reserved,
    /// A field or payload is truncated.
    #[error("truncated acoustic datagram")]
    Truncated,
    /// Datagram or Opus payload exceeds the protocol limit.
    #[error("oversized acoustic datagram")]
    Oversized,
    /// Extra bytes follow the declared payload.
    #[error("acoustic datagram contains trailing bytes")]
    Trailing,
    /// The embedded voice payload is invalid.
    #[error("invalid acoustic voice payload")]
    Voice,
}

/// Client-to-server live voice datagram. Sender identity belongs to the host session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceCapturePacket {
    /// Client stream scoped by the authenticated host session.
    pub stream: VoiceStreamId,
    /// Monotonic packet sequence.
    pub sequence: u32,
    /// Capture sample on the client timeline.
    pub sample_timestamp: u64,
    /// Exact original Opus bytes.
    pub encoded: EncodedVoice,
}

/// Encode one bounded client-to-server voice packet.
pub fn encode_voice_capture(packet: &VoiceCapturePacket) -> Result<Vec<u8>, DatagramError> {
    if packet.stream != packet.encoded.stream || packet.sequence != packet.encoded.sequence {
        return Err(DatagramError::Voice);
    }
    let mut output = header(VOICE_CAPTURE);
    push_u32(&mut output, packet.stream.0);
    push_u32(&mut output, packet.sequence);
    push_u64(&mut output, packet.sample_timestamp);
    push_payload(&mut output, packet.encoded.payload())?;
    finish_encode(output)
}

/// Decode one client voice packet without accepting identity from its payload.
pub fn decode_voice_capture(bytes: &[u8]) -> Result<VoiceCapturePacket, DatagramError> {
    let mut reader = Reader::new(bytes, VOICE_CAPTURE)?;
    let stream = VoiceStreamId(reader.u32()?);
    let sequence = reader.u32()?;
    let sample_timestamp = reader.u64()?;
    let payload = reader.payload()?;
    reader.finish()?;
    let encoded =
        EncodedVoice::new(stream, sequence, payload).map_err(|_error| DatagramError::Voice)?;
    Ok(VoiceCapturePacket {
        stream,
        sequence,
        sample_timestamp,
        encoded,
    })
}

/// Encode one server-gated physical sound delivery for a single connection.
pub fn encode_audible_sound(delivery: &AudibleSoundDelivery) -> Result<Vec<u8>, DatagramError> {
    let mut output = header(AUDIBLE_SOUND);
    push_u32(&mut output, delivery.client_event_id);
    push_u64(&mut output, delivery.play_sample);
    push_propagation(&mut output, delivery.propagation);
    finish_encode(output)
}

/// Decode a delivery and bind it to the recipient selected by the host session.
pub fn decode_audible_sound(
    bytes: &[u8],
    receiver_id: u32,
) -> Result<AudibleSoundDelivery, DatagramError> {
    let mut reader = Reader::new(bytes, AUDIBLE_SOUND)?;
    let client_event_id = reader.u32()?;
    let play_sample = reader.u64()?;
    let propagation = reader.propagation()?;
    reader.finish()?;
    Ok(AudibleSoundDelivery {
        receiver_id,
        client_event_id,
        play_sample,
        propagation,
    })
}

/// Encode one server-gated live voice delivery while preserving Opus exactly.
pub fn encode_audible_voice(delivery: &AudibleVoiceDelivery) -> Result<Vec<u8>, DatagramError> {
    let mut output = header(AUDIBLE_VOICE);
    push_u32(&mut output, delivery.encoded.stream.0);
    push_u32(&mut output, delivery.encoded.sequence);
    push_u64(&mut output, delivery.play_sample);
    push_propagation(&mut output, delivery.propagation);
    push_payload(&mut output, delivery.encoded.payload())?;
    finish_encode(output)
}

/// Decode live voice and bind its recipient from the receiving connection.
pub fn decode_audible_voice(
    bytes: &[u8],
    receiver_id: u32,
) -> Result<AudibleVoiceDelivery, DatagramError> {
    let mut reader = Reader::new(bytes, AUDIBLE_VOICE)?;
    let stream = VoiceStreamId(reader.u32()?);
    let sequence = reader.u32()?;
    let play_sample = reader.u64()?;
    let propagation = reader.propagation()?;
    let payload = reader.payload()?;
    reader.finish()?;
    Ok(AudibleVoiceDelivery {
        receiver_id,
        play_sample,
        propagation,
        encoded: EncodedVoice::new(stream, sequence, payload)
            .map_err(|_error| DatagramError::Voice)?,
    })
}

/// Result of inserting one sequence into the deterministic 60 ms reorder window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoicePacketDisposition {
    /// Packet is retained until preceding sequences arrive or expire.
    Buffered,
    /// Packet duplicates an already released or buffered sequence.
    Duplicate,
    /// Packet is older than the active window.
    Late,
}

/// Three-packet (60 ms at 20 ms/frame) deterministic reorder buffer.
#[derive(Debug)]
pub struct VoiceReorderBuffer {
    expected: u32,
    pending: BTreeMap<u32, VoiceCapturePacket>,
}

impl VoiceReorderBuffer {
    /// Start at the first expected sequence.
    #[must_use]
    pub const fn new(expected: u32) -> Self {
        Self {
            expected,
            pending: BTreeMap::new(),
        }
    }

    /// Insert one packet without accepting duplicates or sequences beyond 60 ms.
    pub fn push(&mut self, packet: VoiceCapturePacket) -> VoicePacketDisposition {
        if packet.sequence < self.expected {
            return VoicePacketDisposition::Late;
        }
        if packet.sequence >= self.expected.saturating_add(3) {
            return VoicePacketDisposition::Late;
        }
        if self.pending.contains_key(&packet.sequence) {
            return VoicePacketDisposition::Duplicate;
        }
        self.pending.insert(packet.sequence, packet);
        VoicePacketDisposition::Buffered
    }

    /// Release the next contiguous packet.
    pub fn pop_ready(&mut self) -> Option<VoiceCapturePacket> {
        let packet = self.pending.remove(&self.expected)?;
        self.expected = self.expected.saturating_add(1);
        Some(packet)
    }

    /// Mark one missing sequence as lost so decoder PLC/FEC can advance.
    pub fn skip_missing(&mut self) {
        self.expected = self.expected.saturating_add(1);
        self.pending
            .retain(|sequence, _packet| *sequence >= self.expected);
    }
}

fn header(kind: u8) -> Vec<u8> {
    let mut output = Vec::with_capacity(MAX_ACOUSTIC_DATAGRAM_BYTES);
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&ACOUSTIC_DATAGRAM_VERSION.to_le_bytes());
    output.push(kind);
    output.push(0);
    output
}

fn finish_encode(output: Vec<u8>) -> Result<Vec<u8>, DatagramError> {
    if output.len() > MAX_ACOUSTIC_DATAGRAM_BYTES {
        Err(DatagramError::Oversized)
    } else {
        Ok(output)
    }
}

fn push_payload(output: &mut Vec<u8>, payload: &[u8]) -> Result<(), DatagramError> {
    let length = u16::try_from(payload.len()).map_err(|_error| DatagramError::Oversized)?;
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(payload);
    Ok(())
}

fn push_propagation(output: &mut Vec<u8>, value: PropagationDescriptor) {
    push_u64(output, value.structure_version.0);
    push_u64(output, value.arrival_sample);
    push_u64(output, value.path_length_mm);
    output.extend_from_slice(&value.gain_db_q8.to_le_bytes());
    for band in value.band_gain.0 {
        output.extend_from_slice(&band.to_le_bytes());
    }
    for direction in value.direction_q15 {
        output.extend_from_slice(&direction.to_le_bytes());
    }
    output.extend_from_slice(&value.uncertainty_q16.to_le_bytes());
    output.push(u8::from(value.direct));
    output.extend_from_slice(&[0; 3]);
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], expected_kind: u8) -> Result<Self, DatagramError> {
        if bytes.len() < HEADER_BYTES {
            return Err(DatagramError::Truncated);
        }
        if bytes.len() > MAX_ACOUSTIC_DATAGRAM_BYTES {
            return Err(DatagramError::Oversized);
        }
        if bytes.get(..4) != Some(MAGIC) {
            return Err(DatagramError::Magic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != ACOUSTIC_DATAGRAM_VERSION {
            return Err(DatagramError::Version(version));
        }
        if bytes[6] != expected_kind {
            return Err(DatagramError::Kind);
        }
        if bytes[7] != 0 {
            return Err(DatagramError::Reserved);
        }
        Ok(Self { bytes, offset: 8 })
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], DatagramError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(DatagramError::Truncated)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(DatagramError::Truncated)?;
        self.offset = end;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, DatagramError> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_error| DatagramError::Truncated)?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn i16(&mut self) -> Result<i16, DatagramError> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_error| DatagramError::Truncated)?;
        Ok(i16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, DatagramError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_error| DatagramError::Truncated)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Result<i32, DatagramError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_error| DatagramError::Truncated)?;
        Ok(i32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, DatagramError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_error| DatagramError::Truncated)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn payload(&mut self) -> Result<&'a [u8], DatagramError> {
        let length = usize::from(self.u16()?);
        if length == 0 {
            return Err(DatagramError::Voice);
        }
        if length > MAX_OPUS_PACKET_BYTES {
            return Err(DatagramError::Oversized);
        }
        self.take(length)
    }

    fn propagation(&mut self) -> Result<PropagationDescriptor, DatagramError> {
        let structure_version = AcousticStructureVersion(self.u64()?);
        let arrival_sample = self.u64()?;
        let path_length_mm = self.u64()?;
        let gain_db_q8 = self.i32()?;
        let band_gain = BandEnergy([self.u16()?, self.u16()?, self.u16()?]);
        let direction_q15 = [self.i16()?, self.i16()?, self.i16()?];
        let uncertainty_q16 = self.u16()?;
        let direct = match self.take(1)?[0] {
            0 => false,
            1 => true,
            _ => return Err(DatagramError::Reserved),
        };
        if self.take(3)? != [0, 0, 0] {
            return Err(DatagramError::Reserved);
        }
        Ok(PropagationDescriptor {
            structure_version,
            arrival_sample,
            path_length_mm,
            gain_db_q8,
            band_gain,
            direction_q15,
            uncertainty_q16,
            direct,
        })
    }

    fn finish(self) -> Result<(), DatagramError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(DatagramError::Trailing)
        }
    }
}

const _: () = assert!(PROPAGATION_BYTES == 46);

#[cfg(test)]
#[path = "../tests/unit/acoustics.rs"]
mod tests;
