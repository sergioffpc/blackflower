use std::num::NonZeroU16;

use blackflower_networking::{InputSequence, ProtocolRevision};
use blackflower_networking_replication::{
    ComponentDescriptor, ComponentId, ComponentRegistry, ProjectionKind, QuantizationError,
    QuantizedPosition, QuantizedQuaternion, QuantizedVelocity, RegistryError, ReplicationPriority,
};

use super::ProtocolError;
use super::wire::{Decoder, ensure_length, put_i16, put_i32, put_u64};

/// Stable revision-1 public transform component identity.
pub const TRANSFORM_COMPONENT_ID: ComponentId = ComponentId::new(NonZeroU16::MIN);
/// Stable revision-1 public velocity component identity.
pub const VELOCITY_COMPONENT_ID: ComponentId = ComponentId::new(NonZeroU16::MIN.saturating_add(1));
/// Stable revision-1 public character-state component identity.
pub const CHARACTER_STATE_COMPONENT_ID: ComponentId =
    ComponentId::new(NonZeroU16::MIN.saturating_add(2));
/// Stable revision-1 owner prediction component identity.
pub const OWNER_PREDICTION_STATE_COMPONENT_ID: ComponentId =
    ComponentId::new(NonZeroU16::MIN.saturating_add(3));

macro_rules! fixed_component_size {
    ($bytes:ident, $maximum:ident, $value:literal) => {
        #[doc = "Exact canonical component byte length."]
        pub const $bytes: usize = $value;
        const $maximum: u16 = $value;
    };
}

fixed_component_size!(TRANSFORM_BYTES, TRANSFORM_MAXIMUM_BYTES, 19);
fixed_component_size!(VELOCITY_BYTES, VELOCITY_MAXIMUM_BYTES, 6);
fixed_component_size!(CHARACTER_STATE_BYTES, CHARACTER_STATE_MAXIMUM_BYTES, 1);
fixed_component_size!(
    OWNER_PREDICTION_STATE_BYTES,
    OWNER_PREDICTION_STATE_MAXIMUM_BYTES,
    9
);

const TRANSFORM_SCHEMA: &str = "transform component v1";
const VELOCITY_SCHEMA: &str = "velocity component v1";
const CHARACTER_STATE_SCHEMA: &str = "character state component v1";
const OWNER_PREDICTION_STATE_SCHEMA: &str = "owner prediction state component v1";

/// Build the complete stable component registry for protocol revision 1.
pub fn component_registry() -> Result<ComponentRegistry, RegistryError> {
    ComponentRegistry::new(
        ProtocolRevision::V1,
        [
            ComponentDescriptor {
                id: TRANSFORM_COMPONENT_ID,
                projection: ProjectionKind::Public,
                maximum_bytes: TRANSFORM_MAXIMUM_BYTES,
            },
            ComponentDescriptor {
                id: VELOCITY_COMPONENT_ID,
                projection: ProjectionKind::Public,
                maximum_bytes: VELOCITY_MAXIMUM_BYTES,
            },
            ComponentDescriptor {
                id: CHARACTER_STATE_COMPONENT_ID,
                projection: ProjectionKind::Public,
                maximum_bytes: CHARACTER_STATE_MAXIMUM_BYTES,
            },
            ComponentDescriptor {
                id: OWNER_PREDICTION_STATE_COMPONENT_ID,
                projection: ProjectionKind::Owner,
                maximum_bytes: OWNER_PREDICTION_STATE_MAXIMUM_BYTES,
            },
        ],
    )
}

/// Return the normative scheduling priority for one revision-1 component.
#[must_use]
pub const fn replication_priority(id: ComponentId) -> Option<ReplicationPriority> {
    let raw = id.get();
    if raw == TRANSFORM_COMPONENT_ID.get()
        || raw == VELOCITY_COMPONENT_ID.get()
        || raw == CHARACTER_STATE_COMPONENT_ID.get()
    {
        Some(ReplicationPriority::ActiveActor)
    } else if raw == OWNER_PREDICTION_STATE_COMPONENT_ID.get() {
        Some(ReplicationPriority::OwnerCorrection)
    } else {
        None
    }
}

/// Canonical revision-1 public world transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transform {
    position: QuantizedPosition,
    orientation: QuantizedQuaternion,
}

impl Transform {
    /// Quantize a finite metre-space position and unit orientation.
    pub fn quantize(
        position_meters: [f64; 3],
        orientation: [f64; 4],
    ) -> Result<Self, QuantizationError> {
        Ok(Self {
            position: QuantizedPosition::quantize(position_meters)?,
            orientation: QuantizedQuaternion::quantize(orientation)?,
        })
    }

    /// Construct an already validated canonical transform.
    #[must_use]
    pub const fn from_quantized(
        position: QuantizedPosition,
        orientation: QuantizedQuaternion,
    ) -> Self {
        Self {
            position,
            orientation,
        }
    }

    /// Return the signed-centimetre position.
    #[must_use]
    pub const fn position(self) -> QuantizedPosition {
        self.position
    }

    /// Return the canonical smallest-three orientation.
    #[must_use]
    pub const fn orientation(self) -> QuantizedQuaternion {
        self.orientation
    }

    /// Encode the exact revision-1 full-replacement bytes.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut output = Vec::with_capacity(TRANSFORM_BYTES);
        for code in self.position.codes() {
            put_i32(&mut output, code);
        }
        output.push(self.orientation.largest_index());
        for component in self.orientation.components() {
            put_i16(&mut output, component);
        }
        output
    }

    /// Decode and validate exact revision-1 transform bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        ensure_length(bytes, TRANSFORM_BYTES, TRANSFORM_SCHEMA)?;
        let mut decoder = Decoder::new(bytes, TRANSFORM_SCHEMA);
        let position =
            QuantizedPosition::from_codes([decoder.i32()?, decoder.i32()?, decoder.i32()?]);
        let orientation = QuantizedQuaternion::try_from_parts(
            decoder.u8()?,
            [decoder.i16()?, decoder.i16()?, decoder.i16()?],
        )?;
        decoder.finish()?;
        Ok(Self {
            position,
            orientation,
        })
    }
}

/// Canonical revision-1 public linear velocity.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Velocity {
    velocity: QuantizedVelocity,
}

impl Velocity {
    /// Quantize finite metres-per-second velocity.
    pub fn quantize(meters_per_second: [f64; 3]) -> Result<Self, QuantizationError> {
        Ok(Self {
            velocity: QuantizedVelocity::quantize(meters_per_second)?,
        })
    }

    /// Construct an already validated canonical velocity.
    #[must_use]
    pub const fn from_quantized(velocity: QuantizedVelocity) -> Self {
        Self { velocity }
    }

    /// Return the signed-centimetre-per-second velocity.
    #[must_use]
    pub const fn velocity(self) -> QuantizedVelocity {
        self.velocity
    }

    /// Encode the exact revision-1 full-replacement bytes.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut output = Vec::with_capacity(VELOCITY_BYTES);
        for code in self.velocity.codes() {
            put_i16(&mut output, code);
        }
        output
    }

    /// Decode exact revision-1 velocity bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        ensure_length(bytes, VELOCITY_BYTES, VELOCITY_SCHEMA)?;
        let mut decoder = Decoder::new(bytes, VELOCITY_SCHEMA);
        let velocity =
            QuantizedVelocity::from_codes([decoder.i16()?, decoder.i16()?, decoder.i16()?]);
        decoder.finish()?;
        Ok(Self { velocity })
    }
}

/// Exact revision-1 public character movement state.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CharacterState {
    grounded: bool,
}

impl CharacterState {
    /// Construct the authoritative grounded state.
    #[must_use]
    pub const fn new(grounded: bool) -> Self {
        Self { grounded }
    }

    /// Return whether the authoritative character is grounded.
    #[must_use]
    pub const fn grounded(self) -> bool {
        self.grounded
    }

    /// Encode the exact canonical boolean byte.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        vec![u8::from(self.grounded)]
    }

    /// Decode the exact canonical boolean byte.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        ensure_length(bytes, CHARACTER_STATE_BYTES, CHARACTER_STATE_SCHEMA)?;
        let grounded = match bytes[0] {
            0 => false,
            1 => true,
            _ => {
                return Err(ProtocolError::InvalidBoolean {
                    field: "character grounded",
                });
            }
        };
        Ok(Self { grounded })
    }
}

/// Exact revision-1 owner-only prediction acknowledgement.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct OwnerPredictionState {
    acknowledged_input: Option<InputSequence>,
}

impl OwnerPredictionState {
    /// Construct the latest canonical input committed by the authoritative simulation.
    #[must_use]
    pub const fn new(acknowledged_input: Option<InputSequence>) -> Self {
        Self { acknowledged_input }
    }

    /// Return the latest committed local input, when one exists.
    #[must_use]
    pub const fn acknowledged_input(self) -> Option<InputSequence> {
        self.acknowledged_input
    }

    /// Encode the exact presence tag and fixed-width input sequence.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut output = Vec::with_capacity(OWNER_PREDICTION_STATE_BYTES);
        output.push(u8::from(self.acknowledged_input.is_some()));
        put_u64(
            &mut output,
            self.acknowledged_input.map_or(0, InputSequence::get),
        );
        output
    }

    /// Decode the exact presence tag and canonical absent representation.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        ensure_length(
            bytes,
            OWNER_PREDICTION_STATE_BYTES,
            OWNER_PREDICTION_STATE_SCHEMA,
        )?;
        let mut decoder = Decoder::new(bytes, OWNER_PREDICTION_STATE_SCHEMA);
        let present = decoder.u8()?;
        let sequence = decoder.u64()?;
        decoder.finish()?;
        let acknowledged_input = match present {
            0 if sequence == 0 => None,
            0 => return Err(ProtocolError::NonCanonicalAbsentInput),
            1 => Some(InputSequence::new(sequence)),
            _ => {
                return Err(ProtocolError::InvalidPresence {
                    field: "acknowledged input",
                });
            }
        };
        Ok(Self { acknowledged_input })
    }
}

/// Any canonical revision-1 gameplay component value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolComponent {
    /// Public transform.
    Transform(Transform),
    /// Public linear velocity.
    Velocity(Velocity),
    /// Public grounded state.
    CharacterState(CharacterState),
    /// Owner-only prediction acknowledgement.
    OwnerPredictionState(OwnerPredictionState),
}

impl ProtocolComponent {
    /// Decode one component selected by its stable revision-1 ID.
    pub fn decode(id: ComponentId, bytes: &[u8]) -> Result<Self, ProtocolError> {
        if id == TRANSFORM_COMPONENT_ID {
            Ok(Self::Transform(Transform::decode(bytes)?))
        } else if id == VELOCITY_COMPONENT_ID {
            Ok(Self::Velocity(Velocity::decode(bytes)?))
        } else if id == CHARACTER_STATE_COMPONENT_ID {
            Ok(Self::CharacterState(CharacterState::decode(bytes)?))
        } else if id == OWNER_PREDICTION_STATE_COMPONENT_ID {
            Ok(Self::OwnerPredictionState(OwnerPredictionState::decode(
                bytes,
            )?))
        } else {
            Err(ProtocolError::UnknownComponent { id: id.get() })
        }
    }

    /// Return the stable revision-1 component ID.
    #[must_use]
    pub const fn id(self) -> ComponentId {
        match self {
            Self::Transform(_value) => TRANSFORM_COMPONENT_ID,
            Self::Velocity(_value) => VELOCITY_COMPONENT_ID,
            Self::CharacterState(_value) => CHARACTER_STATE_COMPONENT_ID,
            Self::OwnerPredictionState(_value) => OWNER_PREDICTION_STATE_COMPONENT_ID,
        }
    }

    /// Encode the selected component's exact full-replacement bytes.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        match self {
            Self::Transform(value) => value.encode(),
            Self::Velocity(value) => value.encode(),
            Self::CharacterState(value) => value.encode(),
            Self::OwnerPredictionState(value) => value.encode(),
        }
    }
}
