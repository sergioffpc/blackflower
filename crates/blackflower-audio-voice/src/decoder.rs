use crate::encoder::frame_length;
use crate::{Channels, Error, FrameDuration, SampleRate, ffi};

/// Stateful decoder for one mono or stereo Opus stream.
#[derive(Debug)]
pub struct Decoder {
    pointer: ffi::DecoderPtr,
    sample_rate: SampleRate,
    channels: Channels,
}

impl Decoder {
    /// Create a new Opus decoder.
    pub fn new(sample_rate: SampleRate, channels: Channels) -> Result<Self, Error> {
        let pointer = ffi::create_decoder(sample_rate, channels)?;
        Ok(Self {
            pointer,
            sample_rate,
            channels,
        })
    }

    /// Decode one Opus packet into the supplied interleaved PCM buffer.
    ///
    /// The returned value is the number of decoded samples per channel.
    pub fn decode(&mut self, packet: &[u8], output: &mut [f32]) -> Result<usize, Error> {
        if packet.is_empty() {
            return Err(Error::EmptyPacket);
        }
        let samples_per_channel = output_capacity(output, self.channels)?;
        ffi::decode_float(
            &mut self.pointer,
            Some(packet),
            output,
            samples_per_channel,
            false,
        )
        .map_err(Error::from)
    }

    /// Decode in-band FEC for one missing frame from the following packet.
    pub fn decode_fec(
        &mut self,
        packet: &[u8],
        duration: FrameDuration,
        output: &mut [f32],
    ) -> Result<usize, Error> {
        if packet.is_empty() {
            return Err(Error::EmptyPacket);
        }
        let samples_per_channel = self.validate_exact_output(duration, output)?;
        ffi::decode_float(
            &mut self.pointer,
            Some(packet),
            output,
            samples_per_channel,
            true,
        )
        .map_err(Error::from)
    }

    /// Generate packet-loss concealment for one missing frame.
    pub fn conceal(&mut self, duration: FrameDuration, output: &mut [f32]) -> Result<usize, Error> {
        let samples_per_channel = self.validate_exact_output(duration, output)?;
        ffi::decode_float(&mut self.pointer, None, output, samples_per_channel, false)
            .map_err(Error::from)
    }

    /// Restore the decoder to its freshly initialized state.
    pub fn reset(&mut self) -> Result<(), Error> {
        ffi::reset_decoder(&mut self.pointer).map_err(Error::from)
    }

    fn validate_exact_output(&self, duration: FrameDuration, output: &[f32]) -> Result<i32, Error> {
        let samples_per_channel = duration.samples_per_channel(self.sample_rate);
        let expected = frame_length(samples_per_channel, self.channels);
        if output.len() != expected {
            return Err(Error::FrameLength {
                buffer: "decoder output",
                expected,
                actual: output.len(),
            });
        }
        i32::try_from(samples_per_channel).map_err(|_error| Error::LengthOutOfRange {
            buffer: "decoder output",
            length: output.len(),
        })
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        ffi::destroy_decoder(&mut self.pointer);
    }
}

fn output_capacity(output: &[f32], channels: Channels) -> Result<i32, Error> {
    if output.is_empty() || !output.len().is_multiple_of(channels.count()) {
        return Err(Error::ChannelAlignment {
            buffer: "decoder output",
            channels: channels.count(),
        });
    }
    let samples_per_channel = output.len() / channels.count();
    i32::try_from(samples_per_channel).map_err(|_error| Error::LengthOutOfRange {
        buffer: "decoder output",
        length: output.len(),
    })
}
