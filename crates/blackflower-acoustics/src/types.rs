use serde::{Deserialize, Serialize};

use crate::Error;

/// Maximum payload accepted from one Opus packet.
pub const MAX_OPUS_PACKET_BYTES: usize = 1_275;

/// A world position quantized to millimetres.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct PositionMm {
    /// Right-handed X coordinate.
    pub x: i32,
    /// Right-handed Y coordinate.
    pub y: i32,
    /// Right-handed Z coordinate.
    pub z: i32,
}

impl PositionMm {
    /// Construct a quantized position.
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Integer Euclidean distance rounded to the nearest millimetre.
    #[must_use]
    pub fn distance(self, other: Self) -> u64 {
        let x = i128::from(self.x) - i128::from(other.x);
        let y = i128::from(self.y) - i128::from(other.y);
        let z = i128::from(self.z) - i128::from(other.z);
        integer_sqrt(u128::try_from(x * x + y * y + z * z).unwrap_or(u128::MAX))
    }
}

fn integer_sqrt(value: u128) -> u64 {
    if value == 0 {
        return 0;
    }
    let mut low = 1_u128;
    let mut high = value.min(u128::from(u64::MAX));
    while low <= high {
        let middle = (low + high) / 2;
        let square = middle.saturating_mul(middle);
        if square == value {
            return u64::try_from(middle).unwrap_or(u64::MAX);
        }
        if square < value {
            low = middle + 1;
        } else {
            high = middle - 1;
        }
    }
    let lower_error = value.saturating_sub(high.saturating_mul(high));
    let upper_error = low.saturating_mul(low).saturating_sub(value);
    let rounded = if upper_error < lower_error { low } else { high };
    u64::try_from(rounded).unwrap_or(u64::MAX)
}

/// Quantized axis-aligned bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AabbMm {
    /// Inclusive lower corner.
    pub min: PositionMm,
    /// Inclusive upper corner.
    pub max: PositionMm,
}

impl AabbMm {
    /// Validate ordered bounds.
    pub fn new(min: PositionMm, max: PositionMm) -> Result<Self, Error> {
        if min.x > max.x || min.y > max.y || min.z > max.z {
            return Err(Error::InvalidField("aabb"));
        }
        Ok(Self { min, max })
    }

    /// Whether a point is inside or on the bounds.
    #[must_use]
    pub const fn contains(self, point: PositionMm) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// Combine two bounds.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            min: PositionMm::new(
                min_i32(self.min.x, other.min.x),
                min_i32(self.min.y, other.min.y),
                min_i32(self.min.z, other.min.z),
            ),
            max: PositionMm::new(
                max_i32(self.max.x, other.max.x),
                max_i32(self.max.y, other.max.y),
                max_i32(self.max.z, other.max.z),
            ),
        }
    }
}

const fn min_i32(left: i32, right: i32) -> i32 {
    if left < right { left } else { right }
}

const fn max_i32(left: i32, right: i32) -> i32 {
    if left > right { left } else { right }
}

/// Low, mid, and high energy or gain in unsigned Q0.16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandEnergy(pub [u16; 3]);

impl BandEnergy {
    /// Unity gain in every band.
    pub const UNITY: Self = Self([u16::MAX; 3]);
    /// Silence in every band.
    pub const SILENT: Self = Self([0; 3]);

    /// Multiply two Q0.16 values with canonical rounding.
    #[must_use]
    pub fn multiplied(self, other: Self) -> Self {
        Self(core::array::from_fn(|index| {
            let product = u32::from(self.0[index]) * u32::from(other.0[index]);
            u16::try_from((product + 32_767) / 65_535).unwrap_or(u16::MAX)
        }))
    }

    /// Largest band value.
    #[must_use]
    pub fn peak(self) -> u16 {
        self.0.into_iter().max().unwrap_or(0)
    }
}

/// Stable structural revision used by one complete acoustic solve.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct AcousticStructureVersion(pub u64);

/// A rigid transform with a millimetre translation and Q1.15 rotation rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantizedTransform {
    /// Local-to-world translation.
    pub translation: PositionMm,
    /// Local-to-world rotation matrix rows in Q1.15.
    pub rotation_q15: [[i16; 3]; 3],
}

impl QuantizedTransform {
    /// Identity rotation at a translated position.
    #[must_use]
    pub const fn translated(translation: PositionMm) -> Self {
        Self {
            translation,
            rotation_q15: [[i16::MAX, 0, 0], [0, i16::MAX, 0], [0, 0, i16::MAX]],
        }
    }

    /// Transform one local point with integer rounding.
    #[must_use]
    pub fn apply(self, local: PositionMm) -> PositionMm {
        let input = [local.x, local.y, local.z];
        let axis = |row: usize| {
            let value = (0..3).fold(0_i64, |total, column| {
                total + i64::from(self.rotation_q15[row][column]) * i64::from(input[column])
            });
            let rounded = (if value >= 0 {
                value + 16_383
            } else {
                value - 16_383
            }) / 32_767;
            i32::try_from(rounded).unwrap_or(if rounded < 0 { i32::MIN } else { i32::MAX })
        };
        PositionMm::new(
            axis(0).saturating_add(self.translation.x),
            axis(1).saturating_add(self.translation.y),
            axis(2).saturating_add(self.translation.z),
        )
    }
}

/// Gameplay classification exposed to hearing and bots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundClass {
    /// Actor locomotion transient.
    Footstep,
    /// Weapon discharge.
    Gunshot,
    /// Live physical-world voice without linguistic interpretation.
    Voice,
    /// Generic collision or material impact.
    Impact,
    /// Explosive event.
    Explosion,
    /// Door, mechanism, or machine.
    Mechanical,
}

/// Opaque voice stream identifier assigned by the host session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VoiceStreamId(pub u32);

/// Fixed-capacity original Opus packet retained for authoritative routing.
#[derive(Clone, PartialEq, Eq)]
pub struct EncodedVoice {
    /// Host-assigned stream.
    pub stream: VoiceStreamId,
    /// Monotonic packet sequence.
    pub sequence: u32,
    len: u16,
    bytes: [u8; MAX_OPUS_PACKET_BYTES],
}

impl core::fmt::Debug for EncodedVoice {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("EncodedVoice")
            .field("stream", &self.stream)
            .field("sequence", &self.sequence)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl EncodedVoice {
    /// Copy a bounded packet into fixed storage.
    pub fn new(stream: VoiceStreamId, sequence: u32, payload: &[u8]) -> Result<Self, Error> {
        if payload.is_empty() || payload.len() > MAX_OPUS_PACKET_BYTES {
            return Err(Error::InvalidField("voice payload"));
        }
        let mut bytes = [0; MAX_OPUS_PACKET_BYTES];
        bytes[..payload.len()].copy_from_slice(payload);
        Ok(Self {
            stream,
            sequence,
            len: u16::try_from(payload.len())
                .map_err(|_error| Error::InvalidField("voice payload"))?,
            bytes,
        })
    }

    /// Exact original Opus bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

/// One canonical sound source captured by the authoritative tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundEmission {
    /// Stable monotonic emission ID used only for deterministic ordering.
    pub id: u64,
    /// Presentation event ID understood by the client.
    pub client_event_id: u32,
    /// Quantized authoritative origin; never copied into deliveries.
    pub position: PositionMm,
    /// Optional authored zone hint.
    pub zone: Option<u32>,
    /// Sample at which the source began.
    pub start_sample: u64,
    /// SPL at one metre in signed Q8.8 decibels.
    pub reference_spl_db_q8: i32,
    /// Source energy distribution.
    pub bands: BandEnergy,
    /// Authored directivity strength in Q0.16.
    pub directivity_q16: u16,
    /// Quantized source-forward direction in Q1.15.
    pub forward_q15: [i16; 3],
    /// Gameplay class.
    pub class: SoundClass,
    /// Authored budget priority.
    pub priority: u8,
    /// Original voice packet when the emission is live voice.
    pub voice: Option<EncodedVoice>,
}

/// One player, bot, or sensor whose audibility is solved independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcousticReceiver {
    /// Stable receiver ID; routing owns the mapping to a connection or bot.
    pub id: u32,
    /// Quantized ear position.
    pub position: PositionMm,
    /// Optional authored zone hint.
    pub zone: Option<u32>,
    /// Hearing threshold in Q8.8 decibels.
    pub threshold_db_q8: i32,
    /// Current masking level in Q8.8 decibels.
    pub masking_db_q8: i32,
    /// Per-band hearing response in Q0.16.
    pub hearing: BandEnergy,
    /// Whether this receiver consumes bot observations instead of client deliveries.
    pub bot: bool,
}

/// Authoritative parameters a client may render without learning the source position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropagationDescriptor {
    /// Acoustic structure used by the solve.
    pub structure_version: AcousticStructureVersion,
    /// Scheduled sample on the server timeline.
    pub arrival_sample: u64,
    /// Quantized resolved path length.
    pub path_length_mm: u64,
    /// Total broadband gain in signed Q8.8 decibels.
    pub gain_db_q8: i32,
    /// Low, mid, and high gain.
    pub band_gain: BandEnergy,
    /// Listener-relative direction in signed Q1.15.
    pub direction_q15: [i16; 3],
    /// Directional uncertainty in Q0.16.
    pub uncertainty_q16: u16,
    /// Whether direct/transmitted geometry was used instead of an alternate portal path.
    pub direct: bool,
}

/// Privacy-preserving acoustic fact available to a bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcousticObservation {
    /// Receiver whose perception memory owns the observation.
    pub receiver_id: u32,
    /// Non-reversible observation token, not the source entity or emission ID.
    pub observation_token: u64,
    /// Coarse gameplay class; voice carries no words or speaker identity.
    pub class: SoundClass,
    /// Sample at which the observation becomes perceptible.
    pub arrival_sample: u64,
    /// Received low, mid, and high energy.
    pub energy: BandEnergy,
    /// Uncertain perceived direction.
    pub direction_q15: [i16; 3],
    /// Uncertainty in Q0.16.
    pub uncertainty_q16: u16,
}

/// Gated physical sound delivery for one player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudibleSoundDelivery {
    /// Recipient resolved outside the wire payload.
    pub receiver_id: u32,
    /// Client presentation event ID.
    pub client_event_id: u32,
    /// Scheduled playback sample.
    pub play_sample: u64,
    /// Authoritative environmental parameters.
    pub propagation: PropagationDescriptor,
}

/// Gated live voice delivery preserving the exact original Opus packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudibleVoiceDelivery {
    /// Recipient resolved outside the wire payload.
    pub receiver_id: u32,
    /// Scheduled playback sample.
    pub play_sample: u64,
    /// Authoritative environmental parameters.
    pub propagation: PropagationDescriptor,
    /// Exact original encoded packet.
    pub encoded: EncodedVoice,
}

/// Complete committed state for one movable acoustic instance or portal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcousticDynamicState {
    /// Instance identifier.
    pub instance_id: u32,
    /// Active prefab state; `None` removes geometry.
    pub state_id: Option<u32>,
    /// Committed rigid transform.
    pub transform: QuantizedTransform,
    /// Optional portal openness in Q0.16.
    pub portal_open_q16: Option<u16>,
}
