use blackflower_networking::{
    CodecViolation, CommandCodec, ControlCodec, InputSequence, ProtocolRevision,
};
use blackflower_networking_protocol::v1::{
    CHARACTER_STATE_BYTES, CHARACTER_STATE_COMPONENT_ID, CharacterState, MOVEMENT_CONTROL_BYTES,
    MovementControl, MovementControlCodec, NoCommandsCodec, OWNER_PREDICTION_STATE_BYTES,
    OWNER_PREDICTION_STATE_COMPONENT_ID, OwnerPredictionState, ProtocolComponent, ProtocolError,
    TRANSFORM_BYTES, TRANSFORM_COMPONENT_ID, Transform, VELOCITY_BYTES, VELOCITY_COMPONENT_ID,
    Velocity, ViewPitch, component_registry, replication_priority,
};
use blackflower_networking_replication::{
    ComponentId, ProjectionKind, QuantizedAngle, QuantizedPosition, QuantizedQuaternion,
    QuantizedVelocity, ReplicationPriority,
};
use bytes::BytesMut;
use glam::{Quat, Vec2, Vec3};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn revision_one_registry_has_stable_ids_visibility_and_bounds() -> TestResult {
    let registry = component_registry()?;
    assert_eq!(registry.revision(), ProtocolRevision::V1);
    let expected = [
        (
            TRANSFORM_COMPONENT_ID,
            1,
            ProjectionKind::Public,
            TRANSFORM_BYTES,
            ReplicationPriority::ActiveActor,
        ),
        (
            VELOCITY_COMPONENT_ID,
            2,
            ProjectionKind::Public,
            VELOCITY_BYTES,
            ReplicationPriority::ActiveActor,
        ),
        (
            CHARACTER_STATE_COMPONENT_ID,
            3,
            ProjectionKind::Public,
            CHARACTER_STATE_BYTES,
            ReplicationPriority::ActiveActor,
        ),
        (
            OWNER_PREDICTION_STATE_COMPONENT_ID,
            4,
            ProjectionKind::Owner,
            OWNER_PREDICTION_STATE_BYTES,
            ReplicationPriority::OwnerCorrection,
        ),
    ];
    for (id, raw, projection, maximum_bytes, priority) in expected {
        assert_eq!(id.get(), raw);
        let descriptor = registry.descriptor(id).ok_or("missing component")?;
        assert_eq!(descriptor.projection, projection);
        assert_eq!(usize::from(descriptor.maximum_bytes), maximum_bytes);
        assert_eq!(replication_priority(id), Some(priority));
    }
    assert!(registry.descriptor(ComponentId::try_from_u16(5)?).is_none());
    assert_eq!(replication_priority(ComponentId::try_from_u16(5)?), None);
    Ok(())
}

#[test]
fn components_match_the_revision_one_golden_vectors() -> TestResult {
    let transform = Transform::from_quantized(
        QuantizedPosition::from_codes([123, -456, 789]),
        QuantizedQuaternion::try_from_parts(3, [0, 0, 0])?,
    );
    let transform_bytes = [
        0x7b, 0x00, 0x00, 0x00, 0x38, 0xfe, 0xff, 0xff, 0x15, 0x03, 0x00, 0x00, 0x03, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];
    assert_eq!(transform.encode().as_ref(), transform_bytes);
    assert_eq!(Transform::decode(&transform_bytes)?, transform);
    let rotated = Transform::quantize(
        Vec3::new(1.25, -2.5, 3.75),
        Quat::from_xyzw(0.1, 0.2, 0.3, 0.9),
    )?;
    assert_eq!(Transform::decode(&rotated.encode())?, rotated);

    let velocity = Velocity::from_quantized(QuantizedVelocity::from_codes([100, -200, 300]));
    let velocity_bytes = [0x64, 0x00, 0x38, 0xff, 0x2c, 0x01];
    assert_eq!(velocity.encode().as_ref(), velocity_bytes);
    assert_eq!(Velocity::decode(&velocity_bytes)?, velocity);

    let character = CharacterState::new(true);
    assert_eq!(character.encode().as_ref(), [1]);
    assert_eq!(CharacterState::decode(&[1])?, character);

    let owner = OwnerPredictionState::new(Some(InputSequence::new(0x0102_0304_0506_0708)));
    let owner_bytes = [1, 8, 7, 6, 5, 4, 3, 2, 1];
    assert_eq!(owner.encode().as_ref(), owner_bytes);
    assert_eq!(OwnerPredictionState::decode(&owner_bytes)?, owner);
    Ok(())
}

#[test]
fn component_dispatch_preserves_id_and_exact_bytes() -> TestResult {
    let value =
        ProtocolComponent::Velocity(Velocity::from_quantized(QuantizedVelocity::from_codes([
            7, 8, 9,
        ])));
    let bytes = value.encode();
    assert_eq!(value.id(), VELOCITY_COMPONENT_ID);
    assert_eq!(ProtocolComponent::decode(value.id(), &bytes)?, value);
    assert!(matches!(
        ProtocolComponent::decode(ComponentId::try_from_u16(99)?, &[]),
        Err(ProtocolError::UnknownComponent { id: 99 })
    ));
    Ok(())
}

#[test]
fn strict_component_decoders_reject_noncanonical_values() -> TestResult {
    assert!(matches!(
        CharacterState::decode(&[2]),
        Err(ProtocolError::InvalidBoolean { .. })
    ));
    assert!(matches!(
        OwnerPredictionState::decode(&[0, 1, 0, 0, 0, 0, 0, 0, 0]),
        Err(ProtocolError::NonCanonicalAbsentInput)
    ));
    assert!(matches!(
        OwnerPredictionState::decode(&[2, 0, 0, 0, 0, 0, 0, 0, 0]),
        Err(ProtocolError::InvalidPresence { .. })
    ));
    let mut invalid_quaternion = BytesMut::zeroed(TRANSFORM_BYTES);
    invalid_quaternion[12] = 4;
    assert!(Transform::decode(&invalid_quaternion).is_err());
    invalid_quaternion[12] = 3;
    invalid_quaternion[13..15].copy_from_slice(&i16::MAX.to_le_bytes());
    invalid_quaternion[15..17].copy_from_slice(&i16::MAX.to_le_bytes());
    invalid_quaternion[17..19].copy_from_slice(&i16::MAX.to_le_bytes());
    assert!(Transform::decode(&invalid_quaternion).is_err());
    assert!(matches!(
        Velocity::decode(&[0; VELOCITY_BYTES - 1]),
        Err(ProtocolError::InvalidLength { .. })
    ));
    Ok(())
}

#[test]
fn movement_control_matches_the_revision_one_golden_vector() -> TestResult {
    let control = MovementControl::from_codes(
        23_170,
        -23_170,
        QuantizedAngle::from_code(16_384),
        ViewPitch::try_from_code(-16_384)?,
    )?;
    let bytes = [0x82, 0x5a, 0x7e, 0xa5, 0x00, 0x40, 0x00, 0xc0];
    assert_eq!(control.encode(), bytes);
    assert_eq!(MovementControl::decode(&bytes)?, control);
    assert_eq!(control.neutralized().move_right_code(), 0);
    assert_eq!(control.neutralized().move_forward_code(), 0);
    assert_eq!(control.neutralized().view_yaw(), control.view_yaw());
    assert_eq!(control.neutralized().view_pitch(), control.view_pitch());
    Ok(())
}

#[test]
fn movement_control_rejects_noncanonical_axes_pitch_and_length() -> TestResult {
    assert!(matches!(
        MovementControl::quantize(Vec2::ONE, 0.0, 0.0),
        Err(ProtocolError::MovementMagnitude)
    ));
    assert!(matches!(
        MovementControl::quantize(Vec2::new(f32::NAN, 0.0), 0.0, 0.0),
        Err(ProtocolError::MovementMagnitude)
    ));
    assert!(matches!(
        MovementControl::quantize(Vec2::ZERO, 0.0, std::f32::consts::PI),
        Err(ProtocolError::InvalidViewPitch)
    ));
    let mut reserved_axis = [0_u8; MOVEMENT_CONTROL_BYTES];
    reserved_axis[..2].copy_from_slice(&i16::MIN.to_le_bytes());
    assert!(matches!(
        MovementControl::decode(&reserved_axis),
        Err(ProtocolError::ReservedMovementAxis)
    ));
    assert!(matches!(
        MovementControl::decode(&[0; MOVEMENT_CONTROL_BYTES - 1]),
        Err(ProtocolError::InvalidLength { .. })
    ));
    Ok(())
}

#[test]
fn generic_codec_boundary_accepts_only_movement_and_no_commands() -> TestResult {
    let control = MovementControl::quantize(Vec2::NEG_Y, std::f32::consts::PI, 0.0)?;
    let codec = MovementControlCodec;
    assert_eq!(codec.protocol_revision(), ProtocolRevision::V1);
    assert_eq!(codec.validate_control(&control.encode()), Ok(()));
    assert_eq!(
        codec.validate_control(&[]),
        Err(CodecViolation::NonCanonical)
    );

    let commands = NoCommandsCodec;
    assert_eq!(commands.protocol_revision(), ProtocolRevision::V1);
    assert_eq!(
        commands.validate_command(1, &[]),
        Err(CodecViolation::UnknownKind)
    );
    Ok(())
}
