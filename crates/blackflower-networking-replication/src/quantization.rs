/// Integer representation of one quantized scalar field.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QuantizedScalar(u32);

impl QuantizedScalar {
    /// Construct a quantized scalar from its protocol code.
    #[must_use]
    pub const fn new(code: u32) -> Self {
        Self(code)
    }

    /// Return the protocol code.
    #[must_use]
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Uniform scalar quantization over one closed finite interval.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScalarQuantizer {
    minimum: f64,
    maximum: f64,
    span: f64,
    bits: u8,
    maximum_code: u32,
}

impl ScalarQuantizer {
    /// Construct a quantizer using between one and 32 protocol bits.
    pub fn new(minimum: f64, maximum: f64, bits: u8) -> Result<Self, QuantizationError> {
        if !(1..=32).contains(&bits) {
            return Err(QuantizationError::InvalidBitCount { bits });
        }
        let span = maximum - minimum;
        if !minimum.is_finite() || !maximum.is_finite() || !span.is_finite() || span <= 0.0 {
            return Err(QuantizationError::InvalidRange { minimum, maximum });
        }
        let maximum_code = match bits {
            32 => u32::MAX,
            _ => (1_u32 << bits) - 1,
        };
        Ok(Self {
            minimum,
            maximum,
            span,
            bits,
            maximum_code,
        })
    }

    /// Return the protocol bit width.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.bits
    }

    /// Return the inclusive source interval.
    #[must_use]
    pub fn range(self) -> [f64; 2] {
        [self.minimum, self.maximum]
    }

    /// Quantize a finite value inside the configured interval.
    pub fn quantize(self, value: f64) -> Result<QuantizedScalar, QuantizationError> {
        if !value.is_finite() {
            return Err(QuantizationError::NonFiniteValue { value });
        }
        if value < self.minimum || value > self.maximum {
            return Err(QuantizationError::ValueOutOfRange {
                value,
                minimum: self.minimum,
                maximum: self.maximum,
            });
        }
        Ok(self.quantize_in_range(value))
    }

    /// Quantize a finite value after clamping it to the configured interval.
    pub fn quantize_clamped(self, value: f64) -> Result<QuantizedScalar, QuantizationError> {
        if !value.is_finite() {
            return Err(QuantizationError::NonFiniteValue { value });
        }
        Ok(self.quantize_in_range(value.clamp(self.minimum, self.maximum)))
    }

    /// Reconstruct the scalar represented by one protocol code.
    pub fn dequantize(self, value: QuantizedScalar) -> Result<f64, QuantizationError> {
        if value.code() > self.maximum_code {
            return Err(QuantizationError::CodeOutOfRange {
                code: value.code(),
                maximum: self.maximum_code,
            });
        }
        if value.code() == 0 {
            return Ok(self.minimum);
        }
        if value.code() == self.maximum_code {
            return Ok(self.maximum);
        }
        let normalized = f64::from(value.code()) / f64::from(self.maximum_code);
        Ok(self.minimum + normalized * self.span)
    }

    fn quantize_in_range(self, value: f64) -> QuantizedScalar {
        let normalized = (value - self.minimum) / self.span;
        let scaled = normalized * f64::from(self.maximum_code);
        QuantizedScalar(rounded_code(scaled))
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the caller bounds and rounds the finite value to the inclusive u32 protocol range"
)]
fn rounded_code(value: f64) -> u32 {
    value.round() as u32
}

/// Three independently configured scalar quantizers for a world position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionQuantizer {
    axes: [ScalarQuantizer; 3],
}

impl PositionQuantizer {
    /// Construct a position quantizer from the X, Y, and Z field policies.
    #[must_use]
    pub const fn new(axes: [ScalarQuantizer; 3]) -> Self {
        Self { axes }
    }

    /// Quantize a position, rejecting any component outside its policy.
    pub fn quantize(self, position: [f64; 3]) -> Result<QuantizedPosition, QuantizationError> {
        Ok(QuantizedPosition([
            self.axes[0].quantize(position[0])?,
            self.axes[1].quantize(position[1])?,
            self.axes[2].quantize(position[2])?,
        ]))
    }

    /// Quantize a position after clamping every finite component.
    pub fn quantize_clamped(
        self,
        position: [f64; 3],
    ) -> Result<QuantizedPosition, QuantizationError> {
        Ok(QuantizedPosition([
            self.axes[0].quantize_clamped(position[0])?,
            self.axes[1].quantize_clamped(position[1])?,
            self.axes[2].quantize_clamped(position[2])?,
        ]))
    }

    /// Reconstruct a position from its protocol representation.
    pub fn dequantize(self, position: QuantizedPosition) -> Result<[f64; 3], QuantizationError> {
        let components = position.components();
        Ok([
            self.axes[0].dequantize(components[0])?,
            self.axes[1].dequantize(components[1])?,
            self.axes[2].dequantize(components[2])?,
        ])
    }
}

/// Quantized protocol representation of a three-dimensional position.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuantizedPosition([QuantizedScalar; 3]);

impl QuantizedPosition {
    /// Return the X, Y, and Z protocol codes.
    #[must_use]
    pub const fn components(self) -> [QuantizedScalar; 3] {
        self.0
    }
}

/// Invalid quantization policy, source value, or protocol code.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum QuantizationError {
    /// Uniform scalar quantization supports one to 32 bits.
    #[error("quantization bit count must be between 1 and 32, got {bits}")]
    InvalidBitCount {
        /// Rejected bit count.
        bits: u8,
    },
    /// Interval endpoints did not describe a finite positive span.
    #[error("quantization range must have a finite positive span, got [{minimum}, {maximum}]")]
    InvalidRange {
        /// Inclusive lower endpoint.
        minimum: f64,
        /// Inclusive upper endpoint.
        maximum: f64,
    },
    /// A source component was not finite.
    #[error("quantization source value must be finite, got {value}")]
    NonFiniteValue {
        /// Rejected source value.
        value: f64,
    },
    /// A source component fell outside the configured interval.
    #[error("quantization source value {value} is outside [{minimum}, {maximum}]")]
    ValueOutOfRange {
        /// Rejected source value.
        value: f64,
        /// Inclusive lower endpoint.
        minimum: f64,
        /// Inclusive upper endpoint.
        maximum: f64,
    },
    /// A protocol code did not fit the configured bit width.
    #[error("quantized code {code} exceeds maximum {maximum}")]
    CodeOutOfRange {
        /// Rejected protocol code.
        code: u32,
        /// Largest code accepted by the quantizer.
        maximum: u32,
    },
}
