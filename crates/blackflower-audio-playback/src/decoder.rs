use blackflower_audio_media::{AudioStreamDecoder, Error};
use kira::Frame;
use kira::sound::streaming::Decoder;

pub(crate) struct KiraStreamDecoder(pub(crate) AudioStreamDecoder);

impl Decoder for KiraStreamDecoder {
    type Error = Error;

    fn sample_rate(&self) -> u32 {
        self.0.sample_rate()
    }

    fn num_frames(&self) -> usize {
        self.0.frame_count()
    }

    fn decode(&mut self) -> Result<Vec<Frame>, Self::Error> {
        self.0.decode().map(|frames| {
            frames
                .into_iter()
                .map(|frame| Frame::new(frame.left, frame.right))
                .collect()
        })
    }

    fn seek(&mut self, index: usize) -> Result<usize, Self::Error> {
        self.0.seek(index)
    }
}
