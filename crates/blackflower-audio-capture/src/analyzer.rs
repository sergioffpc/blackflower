use std::collections::BTreeMap;

use blackflower_acoustics::{BandEnergy, EncodedVoice};
use blackflower_audio_voice::{Channels, Decoder, FrameDuration, SampleRate};

use crate::Error;
use crate::worker::analyze;

const FRAME_SAMPLES: usize = 960;

/// Quantized voice energy retained by authoritative simulation; PCM is not exposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceAcousticFrame {
    /// Mean absolute frame amplitude in Q0.16.
    pub amplitude_q16: u16,
    /// Low, mid, and high distribution.
    pub bands: BandEnergy,
}

/// Per-sender server decoder/analyzer with FEC and PLC entry points.
pub struct VoiceFrameAnalyzer {
    decoder: Decoder,
    pcm: [f32; FRAME_SAMPLES],
}

impl VoiceFrameAnalyzer {
    /// Create one mono 48 kHz analyzer for a host-authenticated sender.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            decoder: Decoder::new(SampleRate::Hz48K, Channels::Mono)?,
            pcm: [0.0; FRAME_SAMPLES],
        })
    }

    /// Decode and immediately reduce one original Opus packet to a spectral fact.
    pub fn analyze(&mut self, packet: &EncodedVoice) -> Result<VoiceAcousticFrame, Error> {
        let decoded = self.decoder.decode(packet.payload(), &mut self.pcm)?;
        Ok(self.finish(decoded))
    }

    /// Recover one missing frame from the following packet when in-band FEC is available.
    pub fn analyze_fec(&mut self, following: &EncodedVoice) -> Result<VoiceAcousticFrame, Error> {
        let decoded =
            self.decoder
                .decode_fec(following.payload(), FrameDuration::Ms20, &mut self.pcm)?;
        Ok(self.finish(decoded))
    }

    /// Generate one packet-loss-concealment fact.
    pub fn conceal(&mut self) -> Result<VoiceAcousticFrame, Error> {
        let decoded = self.decoder.conceal(FrameDuration::Ms20, &mut self.pcm)?;
        Ok(self.finish(decoded))
    }

    fn finish(&mut self, decoded: usize) -> VoiceAcousticFrame {
        let energy = analyze(&self.pcm[..decoded.min(self.pcm.len())]);
        self.pcm.fill(0.0);
        energy
    }
}

/// Bounded decoder/analyzer ownership keyed only by host-authenticated sender ID.
pub struct VoiceAnalyzerBank {
    max_senders: usize,
    senders: BTreeMap<u64, VoiceFrameAnalyzer>,
}

impl VoiceAnalyzerBank {
    /// Create a server-side analyzer pool, normally with capacity 32.
    pub fn new(max_senders: usize) -> Result<Self, Error> {
        if max_senders == 0 {
            return Err(Error::InvalidSetting("voice analyzer capacity"));
        }
        Ok(Self {
            max_senders,
            senders: BTreeMap::new(),
        })
    }

    /// Decode and reduce one exact packet for the authenticated sender.
    pub fn analyze(
        &mut self,
        sender: u64,
        packet: &EncodedVoice,
    ) -> Result<VoiceAcousticFrame, Error> {
        self.sender(sender)?.analyze(packet)
    }

    /// Use the following packet's in-band FEC for one missing frame.
    pub fn analyze_fec(
        &mut self,
        sender: u64,
        following: &EncodedVoice,
    ) -> Result<VoiceAcousticFrame, Error> {
        self.sender(sender)?.analyze_fec(following)
    }

    /// Produce a PLC envelope for one declared missing packet.
    pub fn conceal(&mut self, sender: u64) -> Result<VoiceAcousticFrame, Error> {
        self.sender(sender)?.conceal()
    }

    /// Retire all codec state when a host session ends.
    pub fn remove(&mut self, sender: u64) {
        self.senders.remove(&sender);
    }

    fn sender(&mut self, sender: u64) -> Result<&mut VoiceFrameAnalyzer, Error> {
        let has_capacity = self.senders.len() < self.max_senders;
        match self.senders.entry(sender) {
            std::collections::btree_map::Entry::Occupied(entry) => Ok(entry.into_mut()),
            std::collections::btree_map::Entry::Vacant(entry) if has_capacity => {
                Ok(entry.insert(VoiceFrameAnalyzer::new()?))
            }
            std::collections::btree_map::Entry::Vacant(_entry) => Err(Error::AnalyzerCapacity),
        }
    }
}
