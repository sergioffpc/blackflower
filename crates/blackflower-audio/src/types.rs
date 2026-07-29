use std::num::NonZeroU32;

use glam::Vec3A;

use crate::Error;

/// Global signal-processing settings shared by an HRTF and its effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSettings {
    sampling_rate: NonZeroU32,
    frame_size: NonZeroU32,
}

impl AudioSettings {
    /// Validate a device sampling rate and fixed Steam Audio frame size.
    pub fn new(sampling_rate: u32, frame_size: u32) -> Result<Self, Error> {
        let sampling_rate = NonZeroU32::new(sampling_rate).ok_or(Error::ZeroAudioSetting {
            field: "sampling rate",
        })?;
        let frame_size = NonZeroU32::new(frame_size).ok_or(Error::ZeroAudioSetting {
            field: "frame size",
        })?;
        validate_native_i32("sampling rate", sampling_rate)?;
        validate_native_i32("frame size", frame_size)?;
        Ok(Self {
            sampling_rate,
            frame_size,
        })
    }

    /// Sampling rate in hertz.
    #[must_use]
    pub const fn sampling_rate(self) -> NonZeroU32 {
        self.sampling_rate
    }

    /// Number of samples per channel in one processing frame.
    #[must_use]
    pub const fn frame_size(self) -> NonZeroU32 {
        self.frame_size
    }

    pub(crate) fn raw_sampling_rate(self) -> i32 {
        native_i32(self.sampling_rate)
    }

    pub(crate) fn raw_frame_size(self) -> i32 {
        native_i32(self.frame_size)
    }

    pub(crate) fn frame_len(self) -> usize {
        usize::try_from(self.frame_size.get())
            .unwrap_or_else(|_error| unreachable!("u32 must fit usize on supported targets"))
    }
}

/// HRTF interpolation used while rendering a point source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Interpolation {
    /// Select the nearest measured HRTF direction. This is the cheaper option.
    #[default]
    Nearest,
    /// Blend the four closest measurements for smoother moving sources.
    Bilinear,
}

/// Parameters applied to one frame of binaural point-source audio.
#[derive(Debug, Clone, Copy)]
pub struct BinauralParams {
    direction: Vec3A,
    interpolation: Interpolation,
    spatial_blend: f32,
}

impl BinauralParams {
    /// Create fully spatialized parameters from a listener-relative direction.
    pub fn new(direction: Vec3A) -> Result<Self, Error> {
        let direction = direction
            .try_normalize()
            .filter(|direction| direction.is_finite())
            .ok_or(Error::InvalidDirection)?;
        Ok(Self {
            direction,
            interpolation: Interpolation::Nearest,
            spatial_blend: 1.0,
        })
    }

    /// Select the HRTF interpolation technique.
    #[must_use]
    pub const fn with_interpolation(mut self, interpolation: Interpolation) -> Self {
        self.interpolation = interpolation;
        self
    }

    /// Blend between unspatialized (`0`) and fully spatialized (`1`) output.
    pub fn with_spatial_blend(mut self, spatial_blend: f32) -> Result<Self, Error> {
        if !spatial_blend.is_finite() || !(0.0..=1.0).contains(&spatial_blend) {
            return Err(Error::InvalidSpatialBlend);
        }
        self.spatial_blend = spatial_blend;
        Ok(self)
    }

    pub(crate) const fn direction(self) -> Vec3A {
        self.direction
    }

    pub(crate) const fn interpolation(self) -> Interpolation {
        self.interpolation
    }

    pub(crate) const fn spatial_blend(self) -> f32 {
        self.spatial_blend
    }
}

/// Whether a Steam Audio effect still contains samples after its input ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailState {
    /// Additional tail frames must still be requested.
    Remaining,
    /// No tail samples remain.
    Complete,
}

fn validate_native_i32(field: &'static str, value: NonZeroU32) -> Result<(), Error> {
    i32::try_from(value.get())
        .map(|_value| ())
        .map_err(|_error| Error::AudioSettingOutOfRange {
            field,
            value: value.get(),
        })
}

fn native_i32(value: NonZeroU32) -> i32 {
    i32::try_from(value.get())
        .unwrap_or_else(|_error| unreachable!("AudioSettings validates the native range"))
}
