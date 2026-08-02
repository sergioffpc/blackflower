#![doc = include_str!("../README.md")]

mod asset;
mod error;
mod geometry;
mod solver;
mod types;
mod wire;

pub use asset::{
    ACOUSTIC_ASSET_SCHEMA, AcousticEmissionProfile, AcousticMaterial, AcousticMaterialLibrary,
    AcousticPortal, AcousticPrefab, AcousticPrefabInstance, AcousticSimulationScene,
    AcousticTopology, AcousticZoneVolume, PrefabState, ProbePathEdge, SpectralEnvelopeFrame,
    ZoneResponse,
};
pub use error::Error;
pub use geometry::QuantizedTriangle;
pub use solver::{
    AcousticFrame, AcousticReplayDelivery, AcousticReplayEmission, AcousticReplayFacts,
    AcousticWorld, AcousticWorldSettings,
};
pub use types::{
    AabbMm, AcousticDynamicState, AcousticObservation, AcousticReceiver, AcousticStructureVersion,
    AudibleSoundDelivery, AudibleVoiceDelivery, BandEnergy, EncodedVoice, MAX_OPUS_PACKET_BYTES,
    PositionMm, PropagationDescriptor, QuantizedTransform, SoundClass, SoundEmission,
    VoiceStreamId,
};
pub use wire::{
    ACOUSTIC_DATAGRAM_VERSION, DatagramError, MAX_ACOUSTIC_DATAGRAM_BYTES, VoiceCapturePacket,
    VoicePacketDisposition, VoiceReorderBuffer, decode_audible_sound, decode_audible_voice,
    decode_voice_capture, encode_audible_sound, encode_audible_voice, encode_voice_capture,
};

/// Authoritative acoustic sample rate.
pub const ACOUSTIC_SAMPLE_RATE: u64 = 48_000;
/// Fixed simulation ticks per second.
pub const ACOUSTIC_TICK_RATE: u64 = 240;
/// Samples advanced by one authoritative simulation tick.
pub const SAMPLES_PER_TICK: u64 = ACOUSTIC_SAMPLE_RATE / ACOUSTIC_TICK_RATE;
/// Canonical release sound speed in millimetres per second.
pub const SOUND_SPEED_MM_PER_SECOND: u64 = 343_000;

const _: () = assert!(ACOUSTIC_SAMPLE_RATE.is_multiple_of(ACOUSTIC_TICK_RATE));
