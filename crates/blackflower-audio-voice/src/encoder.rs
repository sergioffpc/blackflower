use crate::{Application, Channels, Error, FrameDuration, SampleRate, ffi};

/// Stateful encoder for one mono or stereo Opus stream.
#[derive(Debug)]
pub struct Encoder {
    pointer: ffi::EncoderPtr,
    sample_rate: SampleRate,
    channels: Channels,
}

impl Encoder {
    /// Create a new Opus encoder.
    pub fn new(
        sample_rate: SampleRate,
        channels: Channels,
        application: Application,
    ) -> Result<Self, Error> {
        let pointer = ffi::create_encoder(sample_rate, channels, application)?;
        Ok(Self {
            pointer,
            sample_rate,
            channels,
        })
    }

    /// Encode one complete interleaved floating-point PCM frame.
    pub fn encode(
        &mut self,
        duration: FrameDuration,
        input: &[f32],
        output: &mut [u8],
    ) -> Result<usize, Error> {
        let samples_per_channel = duration.samples_per_channel(self.sample_rate);
        let expected = frame_length(samples_per_channel, self.channels);
        if input.len() != expected {
            return Err(Error::FrameLength {
                buffer: "encoder input",
                expected,
                actual: input.len(),
            });
        }
        let native_samples =
            i32::try_from(samples_per_channel).map_err(|_error| Error::LengthOutOfRange {
                buffer: "encoder input",
                length: expected,
            })?;
        ffi::encode_float(&mut self.pointer, input, native_samples, output).map_err(Error::from)
    }

    /// Set the target bitrate in bits per second.
    pub fn set_bitrate(&mut self, bits_per_second: u32) -> Result<(), Error> {
        validate_range("bitrate", bits_per_second, 500, 512_000)?;
        let bitrate = i32::try_from(bits_per_second).unwrap_or_else(|_error| {
            unreachable!("validated Opus bitrate must fit a native C int")
        });
        ffi::set_bitrate(&mut self.pointer, bitrate).map_err(Error::from)
    }

    /// Set encoder complexity from `0` (lowest) through `10` (highest).
    pub fn set_complexity(&mut self, complexity: u8) -> Result<(), Error> {
        validate_range("complexity", u32::from(complexity), 0, 10)?;
        ffi::set_complexity(&mut self.pointer, i32::from(complexity)).map_err(Error::from)
    }

    /// Enable or disable variable bitrate encoding.
    pub fn set_vbr(&mut self, enabled: bool) -> Result<(), Error> {
        ffi::set_vbr(&mut self.pointer, enabled).map_err(Error::from)
    }

    /// Enable or disable Opus in-band forward error correction.
    pub fn set_inband_fec(&mut self, enabled: bool) -> Result<(), Error> {
        ffi::set_inband_fec(&mut self.pointer, enabled).map_err(Error::from)
    }

    /// Set the expected packet loss percentage from `0` through `100`.
    pub fn set_expected_packet_loss(&mut self, percentage: u8) -> Result<(), Error> {
        validate_range("expected packet loss", u32::from(percentage), 0, 100)?;
        ffi::set_expected_packet_loss(&mut self.pointer, i32::from(percentage)).map_err(Error::from)
    }

    /// Enable or disable discontinuous transmission for inactive voice.
    pub fn set_dtx(&mut self, enabled: bool) -> Result<(), Error> {
        ffi::set_dtx(&mut self.pointer, enabled).map_err(Error::from)
    }

    /// Restore the encoder to its freshly initialized state.
    pub fn reset(&mut self) -> Result<(), Error> {
        ffi::reset_encoder(&mut self.pointer).map_err(Error::from)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        ffi::destroy_encoder(&mut self.pointer);
    }
}

fn validate_range(
    field: &'static str,
    actual: u32,
    minimum: u32,
    maximum: u32,
) -> Result<(), Error> {
    if (minimum..=maximum).contains(&actual) {
        Ok(())
    } else {
        Err(Error::ConfigurationOutOfRange {
            field,
            minimum,
            maximum,
            actual,
        })
    }
}

pub(crate) fn frame_length(samples_per_channel: u32, channels: Channels) -> usize {
    let samples = usize::try_from(samples_per_channel)
        .unwrap_or_else(|_error| unreachable!("u32 must fit usize on supported targets"));
    samples
        .checked_mul(channels.count())
        .unwrap_or_else(|| unreachable!("supported Opus frames must fit usize"))
}
