#![doc = include_str!("../README.md")]

mod acoustics;
mod harness;

pub use acoustics::{
    ACOUSTIC_DATAGRAM_VERSION, DatagramError, MAX_ACOUSTIC_DATAGRAM_BYTES, VoiceCapturePacket,
    VoicePacketDisposition, VoiceReorderBuffer, decode_audible_sound, decode_audible_voice,
    decode_voice_capture, encode_audible_sound, encode_audible_voice, encode_voice_capture,
};
pub use harness::{HarnessEndpoint, HarnessError, InMemoryDatagramHarness};
