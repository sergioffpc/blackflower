use blackflower_networking::InputSequence;
use blackflower_networking_protocol::v1::{
    CHARACTER_STATE_COMPONENT_ID, CharacterState, OWNER_PREDICTION_STATE_COMPONENT_ID,
    OwnerPredictionState, TRANSFORM_COMPONENT_ID, Transform, VELOCITY_COMPONENT_ID, Velocity,
    replication_priority,
};
use blackflower_networking_replication::{
    ComponentId, ComponentSampleTick, ComponentState, EntityState, ReplicatedEntityId, Snapshot,
    SnapshotTick,
};
use blackflower_world_simulation::{ActorId, MovementFrame};
use bytes::Bytes;

/// Project one sealed movement frame for a particular owning client.
pub fn project_movement_frame(
    frame: &MovementFrame,
    owner: ActorId,
) -> Result<Snapshot, SimulationProjectionError> {
    let sample_tick = ComponentSampleTick::new(frame.tick().get());
    let mut entities = Vec::with_capacity(frame.actors().len());
    for actor in frame.actors() {
        let transform = Transform::quantize(actor.position_meters, actor.orientation)?;
        let velocity = Velocity::quantize(actor.velocity_meters_per_second)?;
        let mut components = vec![
            component(TRANSFORM_COMPONENT_ID, sample_tick, transform.encode())?,
            component(VELOCITY_COMPONENT_ID, sample_tick, velocity.encode())?,
            component(
                CHARACTER_STATE_COMPONENT_ID,
                sample_tick,
                CharacterState::new(actor.grounded).encode(),
            )?,
        ];
        if actor.actor == owner {
            let acknowledged = actor.acknowledged_input_sequence.map(InputSequence::new);
            components.push(component(
                OWNER_PREDICTION_STATE_COMPONENT_ID,
                sample_tick,
                OwnerPredictionState::new(acknowledged).encode(),
            )?);
        }
        entities.push((
            ReplicatedEntityId::try_from_u64(actor.actor.get())?,
            EntityState::new(components)?,
        ));
    }
    Ok(Snapshot::new(
        SnapshotTick::new(frame.tick().get()),
        entities,
    )?)
}

fn component(
    id: ComponentId,
    sample_tick: ComponentSampleTick,
    bytes: Bytes,
) -> Result<(ComponentId, ComponentState), SimulationProjectionError> {
    let priority = replication_priority(id).ok_or(SimulationProjectionError::UnknownComponent)?;
    Ok((id, ComponentState::new(sample_tick, priority, bytes)?))
}

/// Failure while converting sealed simulation state into the v1 projection.
#[derive(Debug, thiserror::Error)]
pub enum SimulationProjectionError {
    /// A simulation value cannot be represented by the normative quantizer.
    #[error(transparent)]
    Quantization(#[from] blackflower_networking_replication::QuantizationError),
    /// An actor identity is invalid in the replication domain.
    #[error(transparent)]
    Identity(#[from] blackflower_networking_replication::IdentityError),
    /// The canonical snapshot rejected an entity or component.
    #[error(transparent)]
    Snapshot(#[from] blackflower_networking_replication::SnapshotError),
    /// The v1 schema omitted a component priority.
    #[error("v1 component is missing its replication priority")]
    UnknownComponent,
}
