use std::f64::consts::{FRAC_1_SQRT_2, TAU};

/// Normative position resolution: one signed centimetre.
pub const POSITION_UNITS_PER_METER: f64 = 100.0;
/// Normative velocity resolution: one signed centimetre per second.
pub const VELOCITY_UNITS_PER_METER_PER_SECOND: f64 = 100.0;

/// Signed centimetre world position on each axis.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuantizedPosition([i32; 3]);

impl QuantizedPosition {
    /// Construct a position from canonical signed-centimetre codes.
    #[must_use]
    pub const fn from_codes(codes: [i32; 3]) -> Self {
        Self(codes)
    }

    /// Quantize a finite metre-space position using signed centimetres.
    pub fn quantize(position_meters: [f64; 3]) -> Result<Self, QuantizationError> {
        Ok(Self([
            position_code(position_meters[0])?,
            position_code(position_meters[1])?,
            position_code(position_meters[2])?,
        ]))
    }

    /// Return signed centimetre codes.
    #[must_use]
    pub const fn codes(self) -> [i32; 3] {
        self.0
    }

    /// Reconstruct metres from signed centimetres.
    #[must_use]
    pub fn dequantize(self) -> [f64; 3] {
        self.0
            .map(|code| f64::from(code) / POSITION_UNITS_PER_METER)
    }
}

/// Signed centimetres-per-second velocity on each axis.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuantizedVelocity([i16; 3]);

impl QuantizedVelocity {
    /// Construct a velocity from canonical signed-centimetre-per-second codes.
    #[must_use]
    pub const fn from_codes(codes: [i16; 3]) -> Self {
        Self(codes)
    }

    /// Quantize a finite velocity using signed centimetres per second.
    pub fn quantize(meters_per_second: [f64; 3]) -> Result<Self, QuantizationError> {
        Ok(Self([
            velocity_code(meters_per_second[0])?,
            velocity_code(meters_per_second[1])?,
            velocity_code(meters_per_second[2])?,
        ]))
    }

    /// Return signed centimetres-per-second codes.
    #[must_use]
    pub const fn codes(self) -> [i16; 3] {
        self.0
    }

    /// Reconstruct metres per second.
    #[must_use]
    pub fn dequantize(self) -> [f64; 3] {
        self.0
            .map(|code| f64::from(code) / VELOCITY_UNITS_PER_METER_PER_SECOND)
    }
}

/// Unsigned 16-bit turn angle with wrap-around canonicalization.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuantizedAngle(u16);

impl QuantizedAngle {
    /// Construct an angle from its canonical unsigned full-turn code.
    #[must_use]
    pub const fn from_code(code: u16) -> Self {
        Self(code)
    }

    /// Quantize radians modulo one full turn.
    pub fn quantize(radians: f64) -> Result<Self, QuantizationError> {
        if !radians.is_finite() {
            return Err(QuantizationError::NonFinite);
        }
        let normalized = radians.rem_euclid(TAU) / TAU;
        let scaled = (normalized * 65_536.0).round();
        Ok(Self(angle_code(scaled)))
    }

    /// Return the unsigned turn code.
    #[must_use]
    pub const fn code(self) -> u16 {
        self.0
    }

    /// Reconstruct radians in the half-open interval `[0, 2pi)`.
    #[must_use]
    pub fn dequantize(self) -> f64 {
        f64::from(self.0) * TAU / 65_536.0
    }
}

/// Canonical smallest-three unit quaternion encoding.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuantizedQuaternion {
    largest_index: u8,
    components: [i16; 3],
}

impl QuantizedQuaternion {
    /// Validate and construct one canonical smallest-three encoding.
    pub fn try_from_parts(
        largest_index: u8,
        components: [i16; 3],
    ) -> Result<Self, QuantizationError> {
        let candidate = Self {
            largest_index,
            components,
        };
        let reconstructed = candidate.dequantize()?;
        if Self::quantize(reconstructed)? == candidate {
            Ok(candidate)
        } else {
            Err(QuantizationError::NonCanonicalQuaternion)
        }
    }

    /// Normalize and encode a quaternion, canonicalizing the omitted term positive.
    pub fn quantize(quaternion: [f64; 4]) -> Result<Self, QuantizationError> {
        if !quaternion.into_iter().all(f64::is_finite) {
            return Err(QuantizationError::NonFinite);
        }
        let magnitude_squared = quaternion
            .into_iter()
            .map(|value| value * value)
            .sum::<f64>();
        if magnitude_squared <= f64::EPSILON {
            return Err(QuantizationError::ZeroQuaternion);
        }
        let inverse_magnitude = magnitude_squared.sqrt().recip();
        let mut normalized = quaternion.map(|value| value * inverse_magnitude);
        let largest_index = largest_component(normalized);
        if normalized[largest_index] < 0.0 {
            normalized = normalized.map(|value| -value);
        }
        let mut components = [0_i16; 3];
        let mut output_index = 0;
        for (index, value) in normalized.into_iter().enumerate() {
            if index != largest_index {
                components[output_index] = quaternion_code(value)?;
                output_index += 1;
            }
        }
        Ok(Self {
            largest_index: u8::try_from(largest_index)
                .map_err(|_error| QuantizationError::OutOfRange)?,
            components,
        })
    }

    /// Return the omitted largest component index from zero through three.
    #[must_use]
    pub const fn largest_index(self) -> u8 {
        self.largest_index
    }

    /// Return the three signed protocol components.
    #[must_use]
    pub const fn components(self) -> [i16; 3] {
        self.components
    }

    /// Reconstruct the canonical positive-largest unit quaternion.
    pub fn dequantize(self) -> Result<[f64; 4], QuantizationError> {
        let largest_index = usize::from(self.largest_index);
        if largest_index >= 4 {
            return Err(QuantizationError::InvalidQuaternionIndex);
        }
        let mut quaternion = [0.0_f64; 4];
        let mut input_index = 0;
        let mut sum = 0.0;
        for (index, output) in quaternion.iter_mut().enumerate() {
            if index != largest_index {
                let value =
                    f64::from(self.components[input_index]) * FRAC_1_SQRT_2 / f64::from(i16::MAX);
                *output = value;
                sum += value * value;
                input_index += 1;
            }
        }
        quaternion[largest_index] = (1.0 - sum).max(0.0).sqrt();
        Ok(quaternion)
    }
}

/// Invalid source value or protocol representation for normative quantization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QuantizationError {
    /// Source value contains NaN or infinity.
    #[error("quantization source must be finite")]
    NonFinite,
    /// Quantized value does not fit the normative signed representation.
    #[error("quantization source is outside the normative range")]
    OutOfRange,
    /// Quaternion magnitude is zero.
    #[error("quaternion magnitude must be non-zero")]
    ZeroQuaternion,
    /// Smallest-three omitted index is outside zero through three.
    #[error("smallest-three quaternion index is invalid")]
    InvalidQuaternionIndex,
    /// Smallest-three bytes do not use the unique canonical encoding.
    #[error("smallest-three quaternion encoding is not canonical")]
    NonCanonicalQuaternion,
}

fn position_code(value: f64) -> Result<i32, QuantizationError> {
    if !value.is_finite() {
        return Err(QuantizationError::NonFinite);
    }
    let scaled = (value * POSITION_UNITS_PER_METER).round();
    if scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
        Err(QuantizationError::OutOfRange)
    } else {
        Ok(f64_to_i32(scaled))
    }
}

fn velocity_code(value: f64) -> Result<i16, QuantizationError> {
    if !value.is_finite() {
        return Err(QuantizationError::NonFinite);
    }
    let scaled = (value * VELOCITY_UNITS_PER_METER_PER_SECOND).round();
    if scaled < f64::from(i16::MIN) || scaled > f64::from(i16::MAX) {
        Err(QuantizationError::OutOfRange)
    } else {
        Ok(f64_to_i16(scaled))
    }
}

fn quaternion_code(value: f64) -> Result<i16, QuantizationError> {
    if !(-FRAC_1_SQRT_2..=FRAC_1_SQRT_2).contains(&value) {
        return Err(QuantizationError::OutOfRange);
    }
    let scaled = (value / FRAC_1_SQRT_2 * f64::from(i16::MAX)).round();
    Ok(f64_to_i16(scaled))
}

fn largest_component(quaternion: [f64; 4]) -> usize {
    let mut largest_index = 0;
    let mut largest_magnitude = quaternion[0].abs();
    for (index, value) in quaternion.into_iter().enumerate().skip(1) {
        let magnitude = value.abs();
        if magnitude > largest_magnitude {
            largest_index = index;
            largest_magnitude = magnitude;
        }
    }
    largest_index
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the caller rounds and bounds the value to the full u16 turn domain"
)]
fn angle_code(value: f64) -> u16 {
    if value >= 65_536.0 { 0 } else { value as u16 }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the caller rounds and bounds the finite value to the inclusive i32 domain"
)]
fn f64_to_i32(value: f64) -> i32 {
    value as i32
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the caller rounds and bounds the finite value to the inclusive i16 domain"
)]
fn f64_to_i16(value: f64) -> i16 {
    value as i16
}
