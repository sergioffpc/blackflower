use crate::Error;

/// A validated normalized time used to sample an animation clip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplingRatio(f32);

impl SamplingRatio {
    /// Construct a finite ratio in the inclusive range `0..=1`.
    pub fn new(ratio: f32) -> Result<Self, Error> {
        if ratio.is_finite() && (0.0..=1.0).contains(&ratio) {
            Ok(Self(ratio))
        } else {
            Err(Error::InvalidSamplingRatio)
        }
    }

    /// Return the normalized sampling time.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}
