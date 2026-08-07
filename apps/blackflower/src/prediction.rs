use std::error::Error as StdError;

use blackflower_ecs::{Component, ComponentId, EntityId, Read, Write};
use blackflower_harness::{ClientPrediction, PredictionCodec, PredictionSession, PredictionUpdate};
use blackflower_networking::{ControlFrame, SimulationTick};
use blackflower_networking_protocol::v1::{
    CHARACTER_STATE_COMPONENT_ID, CharacterState, MovementControl, ORIENTATION_TOLERANCE_RADIANS,
    OWNER_PREDICTION_STATE_COMPONENT_ID, OwnerPredictionState, POSITION_TOLERANCE_METERS,
    ProtocolComponent, TRANSFORM_COMPONENT_ID, Transform, VELOCITY_COMPONENT_ID,
    VELOCITY_TOLERANCE_METERS_PER_SECOND, Velocity,
};
use blackflower_networking_replication::{EntityState, ReplicatedEntityId, Snapshot};
use blackflower_world_prediction::{
    AuthoritativeSnapshot, InputFrame, InputSequence, PREDICTION_TICK_DELTA_SECONDS,
    PredictionDriver, PredictionPass, PredictionPhase, PredictionStateComparison, PredictionTick,
    PredictionWorld,
};
use bytemuck::{Pod, Zeroable};

const PREDICTED_MOVEMENT_SPEED_METERS_PER_SECOND: f64 = 5.0;

/// Latest locally predicted movement state exposed to presentation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PredictedMovementState {
    pub(crate) controlled_entity: ReplicatedEntityId,
    pub(crate) position_meters: [f64; 3],
    pub(crate) velocity_meters_per_second: [f64; 3],
    pub(crate) orientation: [f64; 4],
    pub(crate) grounded: bool,
}

/// Concrete movement prediction used by the built-in native client.
pub(crate) struct ClientMovementPrediction {
    session: PredictionSession<
        MovementPredictionDriver,
        MovementPredictionCodec,
        PredictedMovementState,
        MovementInput,
    >,
}

impl ClientMovementPrediction {
    pub(crate) fn new() -> Result<Self, ClientMovementPredictionError> {
        Ok(Self {
            session: PredictionSession::new(
                MovementPredictionDriver::new().map_err(ClientMovementPredictionError::new)?,
                MovementPredictionCodec,
            ),
        })
    }
}

impl ClientPrediction for ClientMovementPrediction {
    type State = PredictedMovementState;
    type Error = ClientMovementPredictionError;

    fn current_tick(&self) -> SimulationTick {
        self.session.current_tick()
    }

    fn bootstrap(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        self.session
            .bootstrap(snapshot)
            .map_err(ClientMovementPredictionError::new)
    }

    fn apply_snapshot(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        self.session
            .apply_snapshot(snapshot)
            .map_err(ClientMovementPredictionError::new)
    }

    fn queue_control(&mut self, frame: &ControlFrame) -> Result<(), Self::Error> {
        self.session
            .queue_control(frame)
            .map_err(ClientMovementPredictionError::new)
    }

    fn advance_to(&mut self, target: SimulationTick) -> Result<(), Self::Error> {
        self.session
            .advance_to(target)
            .map_err(ClientMovementPredictionError::new)
    }

    fn predicted_state(&self) -> Option<&Self::State> {
        self.session.predicted_state()
    }
}

/// Opaque concrete prediction failure retained as a standard error source.
#[derive(Debug)]
pub(crate) struct ClientMovementPredictionError {
    source: Box<dyn StdError + Send + Sync>,
}

impl ClientMovementPredictionError {
    fn new(error: impl StdError + Send + Sync + 'static) -> Self {
        Self {
            source: Box::new(error),
        }
    }
}

impl std::fmt::Display for ClientMovementPredictionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("client movement prediction failed")
    }
}

impl StdError for ClientMovementPredictionError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Debug, Clone)]
struct MovementInput {
    movement: [f64; 2],
    view: Option<[f64; 2]>,
}

#[derive(Debug, Default, Clone, Copy)]
struct MovementPredictionCodec;

impl PredictionCodec<PredictedMovementState, MovementInput> for MovementPredictionCodec {
    type Error = MovementPredictionCodecError;

    fn decode_snapshot(
        &mut self,
        snapshot: &Snapshot,
    ) -> Result<AuthoritativeSnapshot<PredictedMovementState>, Self::Error> {
        validate_components(snapshot)?;
        let (controlled_entity, state) = controlled_state(snapshot)?;
        let transform = required_component(state, TRANSFORM_COMPONENT_ID, "transform")?;
        let velocity = required_component(state, VELOCITY_COMPONENT_ID, "velocity")?;
        let character = required_component(state, CHARACTER_STATE_COMPONENT_ID, "character state")?;
        let owner = required_component(
            state,
            OWNER_PREDICTION_STATE_COMPONENT_ID,
            "owner prediction state",
        )?;
        let transform = Transform::decode(transform)?;
        let state = PredictedMovementState {
            controlled_entity,
            position_meters: transform.position().dequantize(),
            velocity_meters_per_second: Velocity::decode(velocity)?.velocity().dequantize(),
            orientation: transform.orientation().dequantize()?,
            grounded: CharacterState::decode(character)?.grounded(),
        };
        let acknowledged = OwnerPredictionState::decode(owner)?
            .acknowledged_input()
            .map(|sequence| InputSequence::new(sequence.get()));
        Ok(AuthoritativeSnapshot {
            tick: PredictionTick::new(snapshot.tick().get()),
            acknowledged_input: acknowledged,
            state,
        })
    }

    fn decode_input(&mut self, frame: &ControlFrame) -> Result<MovementInput, Self::Error> {
        let control = MovementControl::decode(&frame.payload)?;
        Ok(MovementInput {
            movement: control.movement(),
            view: Some([
                control.view_yaw().dequantize(),
                control.view_pitch().dequantize(),
            ]),
        })
    }

    fn neutral_input(&self) -> MovementInput {
        MovementInput {
            movement: [0.0; 2],
            view: None,
        }
    }

    fn compare_states(
        &self,
        predicted: &PredictedMovementState,
        authoritative: &PredictedMovementState,
    ) -> PredictionStateComparison {
        PredictionStateComparison::from_within_tolerance(
            predicted.controlled_entity == authoritative.controlled_entity
                && predicted.grounded == authoritative.grounded
                && vector_within(
                    predicted.position_meters,
                    authoritative.position_meters,
                    POSITION_TOLERANCE_METERS,
                )
                && vector_within(
                    predicted.velocity_meters_per_second,
                    authoritative.velocity_meters_per_second,
                    VELOCITY_TOLERANCE_METERS_PER_SECOND,
                )
                && quaternion_distance(predicted.orientation, authoritative.orientation)
                    <= ORIENTATION_TOLERANCE_RADIANS,
        )
    }
}

#[derive(Debug, thiserror::Error)]
enum MovementPredictionCodecError {
    #[error(transparent)]
    Protocol(#[from] blackflower_networking_protocol::v1::ProtocolError),
    #[error(transparent)]
    Quantization(#[from] blackflower_networking_replication::QuantizationError),
    #[error("authoritative projection has no controlled movement entity")]
    MissingControlledEntity,
    #[error("authoritative projection has more than one controlled movement entity")]
    DuplicateControlledEntity,
    #[error("controlled entity is missing {0}")]
    MissingComponent(&'static str),
}

fn validate_components(snapshot: &Snapshot) -> Result<(), MovementPredictionCodecError> {
    for (_entity, state) in snapshot.entities() {
        for (id, component) in state.components() {
            let _decoded = ProtocolComponent::decode(id, component.bytes())?;
        }
    }
    Ok(())
}

fn controlled_state(
    snapshot: &Snapshot,
) -> Result<(ReplicatedEntityId, &EntityState), MovementPredictionCodecError> {
    let mut controlled = None;
    for (entity, state) in snapshot.entities() {
        if state.get(OWNER_PREDICTION_STATE_COMPONENT_ID).is_some()
            && controlled.replace((entity, state)).is_some()
        {
            return Err(MovementPredictionCodecError::DuplicateControlledEntity);
        }
    }
    controlled.ok_or(MovementPredictionCodecError::MissingControlledEntity)
}

fn required_component<'a>(
    state: &'a EntityState,
    id: blackflower_networking_replication::ComponentId,
    name: &'static str,
) -> Result<&'a [u8], MovementPredictionCodecError> {
    state
        .get(id)
        .map(blackflower_networking_replication::ComponentState::bytes)
        .ok_or(MovementPredictionCodecError::MissingComponent(name))
}

fn vector_within<const N: usize>(left: [f64; N], right: [f64; N], tolerance: f64) -> bool {
    left.into_iter().zip(right).all(|(left, right)| {
        left.is_finite() && right.is_finite() && (left - right).abs() <= tolerance
    })
}

fn quaternion_distance(left: [f64; 4], right: [f64; 4]) -> f64 {
    let dot = left
        .into_iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum::<f64>()
        .abs()
        .clamp(0.0, 1.0);
    2.0 * dot.acos()
}

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct PredictedPosition([f64; 3]);

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct PredictedVelocity([f64; 3]);

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct PredictedOrientation([f64; 4]);

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct PredictedGrounded(u32);

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(C)]
struct PredictedInput {
    movement: [f64; 2],
    view_yaw_radians: f64,
    view_pitch_radians: f64,
    replace_view: u64,
}

struct MovementPredictionDriver {
    world: PredictionWorld,
    entity: EntityId,
    position: ComponentId<PredictedPosition>,
    velocity: ComponentId<PredictedVelocity>,
    orientation: ComponentId<PredictedOrientation>,
    grounded: ComponentId<PredictedGrounded>,
    input: ComponentId<PredictedInput>,
    controlled_entity: Option<ReplicatedEntityId>,
}

impl MovementPredictionDriver {
    fn new() -> Result<Self, MovementPredictionDriverError> {
        let mut world = PredictionWorld::new()?;
        let position = world.ecs_mut().register_component::<PredictedPosition>()?;
        let velocity = world.ecs_mut().register_component::<PredictedVelocity>()?;
        let orientation = world
            .ecs_mut()
            .register_component::<PredictedOrientation>()?;
        let grounded = world.ecs_mut().register_component::<PredictedGrounded>()?;
        let input = world.ecs_mut().register_component::<PredictedInput>()?;
        let entity = world.ecs_mut().spawn()?;
        world
            .ecs_mut()
            .insert(entity, position, PredictedPosition::zeroed())?;
        world
            .ecs_mut()
            .insert(entity, velocity, PredictedVelocity::zeroed())?;
        world.ecs_mut().insert(
            entity,
            orientation,
            PredictedOrientation([0.0, 0.0, 0.0, 1.0]),
        )?;
        world
            .ecs_mut()
            .insert(entity, grounded, PredictedGrounded(1))?;
        world
            .ecs_mut()
            .insert(entity, input, PredictedInput::zeroed())?;
        register_movement_system(&mut world)?;
        Ok(Self {
            world,
            entity,
            position,
            velocity,
            orientation,
            grounded,
            input,
            controlled_entity: None,
        })
    }

    fn state(&self) -> Result<PredictedMovementState, MovementPredictionDriverError> {
        Ok(PredictedMovementState {
            controlled_entity: self
                .controlled_entity
                .ok_or(MovementPredictionDriverError::MissingControlledEntity)?,
            position_meters: self
                .world
                .ecs()
                .get(self.entity, self.position)?
                .ok_or(MovementPredictionDriverError::MissingState)?
                .0,
            velocity_meters_per_second: self
                .world
                .ecs()
                .get(self.entity, self.velocity)?
                .ok_or(MovementPredictionDriverError::MissingState)?
                .0,
            orientation: self
                .world
                .ecs()
                .get(self.entity, self.orientation)?
                .ok_or(MovementPredictionDriverError::MissingState)?
                .0,
            grounded: self
                .world
                .ecs()
                .get(self.entity, self.grounded)?
                .ok_or(MovementPredictionDriverError::MissingState)?
                .0
                != 0,
        })
    }
}

impl PredictionDriver<PredictedMovementState, InputFrame<MovementInput>>
    for MovementPredictionDriver
{
    type Error = MovementPredictionDriverError;

    fn current_tick(&self) -> u64 {
        self.world.current_tick().get()
    }

    fn restore_authoritative(
        &mut self,
        tick: u64,
        state: &PredictedMovementState,
    ) -> Result<(), Self::Error> {
        self.world.ecs_mut().insert(
            self.entity,
            self.position,
            PredictedPosition(state.position_meters),
        )?;
        self.world.ecs_mut().insert(
            self.entity,
            self.velocity,
            PredictedVelocity(state.velocity_meters_per_second),
        )?;
        self.world.ecs_mut().insert(
            self.entity,
            self.orientation,
            PredictedOrientation(state.orientation),
        )?;
        self.world.ecs_mut().insert(
            self.entity,
            self.grounded,
            PredictedGrounded(u32::from(state.grounded)),
        )?;
        self.controlled_entity = Some(state.controlled_entity);
        self.world
            .restore_tick_for_reconciliation(PredictionTick::new(tick));
        Ok(())
    }

    fn simulate_tick(
        &mut self,
        pass: PredictionPass,
        tick: u64,
        input: &InputFrame<MovementInput>,
    ) -> Result<PredictedMovementState, Self::Error> {
        if input.tick().get() != tick || self.current_tick().checked_add(1) != Some(tick) {
            return Err(MovementPredictionDriverError::TickMismatch);
        }
        let (view_yaw_radians, view_pitch_radians, replace_view) = input
            .input()
            .view
            .map_or((0.0, 0.0, 0), |view| (view[0], view[1], 1));
        self.world.ecs_mut().insert(
            self.entity,
            self.input,
            PredictedInput {
                movement: input.input().movement,
                view_yaw_radians,
                view_pitch_radians,
                replace_view,
            },
        )?;
        let _continue = self.world.tick(pass)?;
        self.state()
    }
}

#[derive(Debug, thiserror::Error)]
enum MovementPredictionDriverError {
    #[error(transparent)]
    Ecs(#[from] blackflower_ecs::Error),
    #[error(transparent)]
    Prediction(#[from] blackflower_world_prediction::PredictionError),
    #[error("predicted movement state is incomplete")]
    MissingState,
    #[error("predicted movement has no controlled entity")]
    MissingControlledEntity,
    #[error("prediction driver tick does not match the selected input")]
    TickMismatch,
}

fn register_movement_system(world: &mut PredictionWorld) -> Result<(), blackflower_ecs::Error> {
    let phase = world.phase(PredictionPhase::SolveRigidBodyDynamics);
    world
        .ecs_mut()
        .system(
            "IntegratePredictedMovement",
            "PredictedPosition, PredictedVelocity, PredictedOrientation, PredictedInput",
        )?
        .phase(phase)?
        .project((
            Write::<PredictedPosition>::field(0),
            Write::<PredictedVelocity>::field(1),
            Write::<PredictedOrientation>::field(2),
            Read::<PredictedInput>::field(3),
        ))?
        .each(
            |_context, _entity, (position, velocity, orientation, input)| {
                integrate_prediction(position, velocity, orientation, input);
                Ok(())
            },
        )?;
    Ok(())
}

fn integrate_prediction(
    position: &mut PredictedPosition,
    velocity: &mut PredictedVelocity,
    orientation: &mut PredictedOrientation,
    input: &PredictedInput,
) {
    let view = if input.replace_view == 0 {
        view_from_orientation(orientation.0)
    } else {
        [input.view_yaw_radians, input.view_pitch_radians]
    };
    let (sine, cosine) = view[0].sin_cos();
    let right = [cosine, 0.0, -sine];
    let forward = [-sine, 0.0, -cosine];
    velocity.0 = [
        (right[0] * input.movement[0] + forward[0] * input.movement[1])
            * PREDICTED_MOVEMENT_SPEED_METERS_PER_SECOND,
        0.0,
        (right[2] * input.movement[0] + forward[2] * input.movement[1])
            * PREDICTED_MOVEMENT_SPEED_METERS_PER_SECOND,
    ];
    let delta = f64::from(PREDICTION_TICK_DELTA_SECONDS);
    for (position, velocity) in position.0.iter_mut().zip(velocity.0) {
        *position += velocity * delta;
    }
    orientation.0 = orientation_from_view(view);
}

fn orientation_from_view([yaw, pitch]: [f64; 2]) -> [f64; 4] {
    let (pitch_sine, pitch_cosine) = (pitch * 0.5).sin_cos();
    let (yaw_sine, yaw_cosine) = (yaw * 0.5).sin_cos();
    [
        yaw_cosine * pitch_sine,
        yaw_sine * pitch_cosine,
        -yaw_sine * pitch_sine,
        yaw_cosine * pitch_cosine,
    ]
}

fn view_from_orientation([x, y, z, w]: [f64; 4]) -> [f64; 2] {
    let pitch = (2.0 * (w * x - y * z)).clamp(-1.0, 1.0).asin();
    let yaw = (2.0 * (w * y - x * z))
        .atan2(1.0 - 2.0 * (x * x + y * y))
        .rem_euclid(std::f64::consts::TAU);
    [yaw, pitch]
}

#[cfg(test)]
#[path = "../tests/unit/prediction.rs"]
mod tests;
