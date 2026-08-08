use std::f32::consts::FRAC_PI_2;

use blackflower_networking::{CodecViolation, CommandCodec, ControlCodec, ProtocolRevision};
use blackflower_networking_replication::QuantizedAngle;
use glam::Vec2;

use super::ProtocolError;
use super::wire::{Decoder, ensure_length};

/// Exact revision-1 movement-control byte length.
pub const MOVEMENT_CONTROL_BYTES: usize = 8;
/// Largest positive canonical movement-axis code.
pub const MOVEMENT_AXIS_CODE_LIMIT: i16 = i16::MAX;

const MOVEMENT_CONTROL_SCHEMA: &str = "movement control v1";
const MOVEMENT_AXIS_SCALE: f32 = 32_767.0;
const MOVEMENT_VECTOR_SQUARED_LIMIT: i64 = 32_768_i64 * 32_768_i64;
const VIEW_PITCH_SCALE: f32 = 32_767.0;

/// Canonical signed absolute view pitch in the closed `[-pi/2, pi/2]` range.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ViewPitch(i16);

impl ViewPitch {
    /// Quantize a finite pitch in radians.
    pub fn quantize(radians: f32) -> Result<Self, ProtocolError> {
        if !(-FRAC_PI_2..=FRAC_PI_2).contains(&radians) {
            return Err(ProtocolError::InvalidViewPitch);
        }
        let scaled = (radians / FRAC_PI_2 * VIEW_PITCH_SCALE).round();
        Ok(Self(f32_to_pitch_code(scaled)))
    }

    /// Validate and construct a canonical signed pitch code.
    pub const fn try_from_code(code: i16) -> Result<Self, ProtocolError> {
        if code == i16::MIN {
            Err(ProtocolError::InvalidViewPitch)
        } else {
            Ok(Self(code))
        }
    }

    /// Return the signed pitch code.
    #[must_use]
    pub const fn code(self) -> i16 {
        self.0
    }

    /// Reconstruct pitch radians.
    #[must_use]
    pub fn dequantize(self) -> f32 {
        f32::from(self.0) * FRAC_PI_2 / VIEW_PITCH_SCALE
    }
}

/// Canonical movement intent and absolute view orientation produced at 60 Hz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MovementControl {
    move_right: i16,
    move_forward: i16,
    view_yaw: QuantizedAngle,
    view_pitch: ViewPitch,
}

impl MovementControl {
    /// Quantize normalized local-space movement and absolute view angles.
    pub fn quantize(
        movement: Vec2,
        view_yaw_radians: f32,
        view_pitch_radians: f32,
    ) -> Result<Self, ProtocolError> {
        let move_right = quantize_axis(movement.x)?;
        let move_forward = quantize_axis(movement.y)?;
        Self::from_codes(
            move_right,
            move_forward,
            QuantizedAngle::quantize(view_yaw_radians)?,
            ViewPitch::quantize(view_pitch_radians)?,
        )
    }

    /// Validate and construct canonical movement and orientation codes.
    pub fn from_codes(
        move_right: i16,
        move_forward: i16,
        view_yaw: QuantizedAngle,
        view_pitch: ViewPitch,
    ) -> Result<Self, ProtocolError> {
        validate_axes(move_right, move_forward)?;
        Ok(Self {
            move_right,
            move_forward,
            view_yaw,
            view_pitch,
        })
    }

    /// Return signed rightward movement in the canonical integer domain.
    #[must_use]
    pub const fn move_right_code(self) -> i16 {
        self.move_right
    }

    /// Return signed forward movement in the canonical integer domain.
    #[must_use]
    pub const fn move_forward_code(self) -> i16 {
        self.move_forward
    }

    /// Return normalized rightward and forward movement values.
    #[must_use]
    pub fn movement(self) -> Vec2 {
        Vec2::new(
            f32::from(self.move_right) / MOVEMENT_AXIS_SCALE,
            f32::from(self.move_forward) / MOVEMENT_AXIS_SCALE,
        )
    }

    /// Return the absolute full-turn view yaw.
    #[must_use]
    pub const fn view_yaw(self) -> QuantizedAngle {
        self.view_yaw
    }

    /// Return the absolute bounded view pitch.
    #[must_use]
    pub const fn view_pitch(self) -> ViewPitch {
        self.view_pitch
    }

    /// Produce neutral movement while retaining the last absolute orientation.
    #[must_use]
    pub const fn neutralized(self) -> Self {
        Self {
            move_right: 0,
            move_forward: 0,
            view_yaw: self.view_yaw,
            view_pitch: self.view_pitch,
        }
    }

    /// Encode the exact eight-byte revision-1 payload.
    #[must_use]
    pub fn encode(self) -> [u8; MOVEMENT_CONTROL_BYTES] {
        let move_right = self.move_right.to_le_bytes();
        let move_forward = self.move_forward.to_le_bytes();
        let view_yaw = self.view_yaw.code().to_le_bytes();
        let view_pitch = self.view_pitch.code().to_le_bytes();
        [
            move_right[0],
            move_right[1],
            move_forward[0],
            move_forward[1],
            view_yaw[0],
            view_yaw[1],
            view_pitch[0],
            view_pitch[1],
        ]
    }

    /// Decode and validate the exact revision-1 payload.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        ensure_length(bytes, MOVEMENT_CONTROL_BYTES, MOVEMENT_CONTROL_SCHEMA)?;
        let mut decoder = Decoder::new(bytes, MOVEMENT_CONTROL_SCHEMA);
        let move_right = decoder.i16()?;
        let move_forward = decoder.i16()?;
        let view_yaw = QuantizedAngle::from_code(decoder.u16()?);
        let view_pitch = ViewPitch::try_from_code(decoder.i16()?)?;
        decoder.finish()?;
        Self::from_codes(move_right, move_forward, view_yaw, view_pitch)
    }
}

/// Revision-1 validator registered at the generic control envelope boundary.
#[derive(Debug, Default, Clone, Copy)]
pub struct MovementControlCodec;

impl ControlCodec for MovementControlCodec {
    fn protocol_revision(&self) -> ProtocolRevision {
        ProtocolRevision::V1
    }

    fn validate_control(&self, bytes: &[u8]) -> Result<(), CodecViolation> {
        MovementControl::decode(bytes)
            .map(|_control| ())
            .map_err(|_error| CodecViolation::NonCanonical)
    }
}

/// Revision-1 command registry: movement and orientation define no discrete commands.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoCommandsCodec;

impl CommandCodec for NoCommandsCodec {
    fn protocol_revision(&self) -> ProtocolRevision {
        ProtocolRevision::V1
    }

    fn validate_command(&self, _kind: u16, _bytes: &[u8]) -> Result<(), CodecViolation> {
        Err(CodecViolation::UnknownKind)
    }
}

fn validate_axes(move_right: i16, move_forward: i16) -> Result<(), ProtocolError> {
    if move_right == i16::MIN || move_forward == i16::MIN {
        return Err(ProtocolError::ReservedMovementAxis);
    }
    let right = i64::from(move_right);
    let forward = i64::from(move_forward);
    if right * right + forward * forward > MOVEMENT_VECTOR_SQUARED_LIMIT {
        Err(ProtocolError::MovementMagnitude)
    } else {
        Ok(())
    }
}

fn quantize_axis(value: f32) -> Result<i16, ProtocolError> {
    if !value.is_finite() || !(-1.0..=1.0).contains(&value) {
        return Err(ProtocolError::MovementMagnitude);
    }
    Ok(f32_to_axis_code((value * MOVEMENT_AXIS_SCALE).round()))
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the caller rounds and bounds the finite value to the inclusive movement-axis domain"
)]
fn f32_to_axis_code(value: f32) -> i16 {
    value as i16
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the caller rounds and bounds the finite value to the canonical pitch domain"
)]
fn f32_to_pitch_code(value: f32) -> i16 {
    value as i16
}
