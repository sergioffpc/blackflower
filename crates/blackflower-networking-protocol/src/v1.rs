//! Concrete component and control schema for protocol revision 1.

mod component;
mod control;
mod error;
mod policy;
mod wire;

pub use component::{
    CHARACTER_STATE_BYTES, CHARACTER_STATE_COMPONENT_ID, CharacterState,
    OWNER_PREDICTION_STATE_BYTES, OWNER_PREDICTION_STATE_COMPONENT_ID, OwnerPredictionState,
    ProtocolComponent, TRANSFORM_BYTES, TRANSFORM_COMPONENT_ID, Transform, VELOCITY_BYTES,
    VELOCITY_COMPONENT_ID, Velocity, component_registry, replication_priority,
};
pub use control::{
    MOVEMENT_AXIS_CODE_LIMIT, MOVEMENT_CONTROL_BYTES, MovementControl, MovementControlCodec,
    NoCommandsCodec, ViewPitch,
};
pub use error::ProtocolError;
pub use policy::{
    ORIENTATION_TOLERANCE_RADIANS, POSITION_TOLERANCE_METERS, VELOCITY_TOLERANCE_METERS_PER_SECOND,
};
