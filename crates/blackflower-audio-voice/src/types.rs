/// Sampling rate accepted by the Opus encoder and decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRate {
    Hz8K,
    Hz12K,
    Hz16K,
    Hz24K,
    Hz48K,
}

impl SampleRate {
    /// Sampling rate in hertz.
    #[must_use]
    pub const fn hertz(self) -> u32 {
        match self {
            Self::Hz8K => 8_000,
            Self::Hz12K => 12_000,
            Self::Hz16K => 16_000,
            Self::Hz24K => 24_000,
            Self::Hz48K => 48_000,
        }
    }

    pub(crate) const fn native(self) -> i32 {
        match self {
            Self::Hz8K => 8_000,
            Self::Hz12K => 12_000,
            Self::Hz16K => 16_000,
            Self::Hz24K => 24_000,
            Self::Hz48K => 48_000,
        }
    }
}

/// Number of interleaved channels encoded in one Opus stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    Mono,
    Stereo,
}

impl Channels {
    /// Number of channels.
    #[must_use]
    pub const fn count(self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }

    pub(crate) const fn native(self) -> i32 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }
}

/// Signal profile used to initialize an Opus encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Application {
    Voip,
    Audio,
    RestrictedLowDelay,
}

/// Duration of one Opus input frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDuration {
    Ms2_5,
    Ms5,
    Ms10,
    Ms20,
    Ms40,
    Ms60,
}

impl FrameDuration {
    /// Number of samples per channel at the selected sampling rate.
    #[must_use]
    pub const fn samples_per_channel(self, sample_rate: SampleRate) -> u32 {
        let rate = sample_rate.hertz();
        match self {
            Self::Ms2_5 => rate / 400,
            Self::Ms5 => rate / 200,
            Self::Ms10 => rate / 100,
            Self::Ms20 => rate / 50,
            Self::Ms40 => rate / 25,
            Self::Ms60 => (rate / 50) * 3,
        }
    }
}
