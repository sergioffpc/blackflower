use blackflower_ecs::{Component, ComponentId, EntityId, Read, Write};
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
use glam::{EulerRot, Quat, Vec2, Vec3};

use crate::{ClientPrediction, PredictionCodec, PredictionSession, PredictionUpdate};

const PREDICTED_MOVEMENT_SPEED_METERS_PER_SECOND: f32 = 5.0;

/// Latest locally predicted movement state exposed to presentation.
#[derive(Debug, Clone, PartialEq)]
pub struct PredictedMovementState {
    /// Server-assigned entity represented by this locally predicted state.
    pub controlled_entity: ReplicatedEntityId,
    /// Predicted world-space position in meters.
    pub position_meters: Vec3,
    /// Predicted world-space velocity in meters per second.
    pub velocity_meters_per_second: Vec3,
    /// Predicted world-space orientation.
    pub orientation: Quat,
    /// Whether the predicted character is grounded.
    pub grounded: bool,
}

/// Concrete movement prediction shared by native and headless clients.
pub struct ClientMovementPrediction {
    session: PredictionSession<
        MovementPredictionDriver,
        MovementPredictionCodec,
        PredictedMovementState,
        MovementInput,
    >,
}

impl ClientMovementPrediction {
    /// Create the revision-one movement prediction world and session.
    pub fn new() -> Result<Self, ClientMovementPredictionError> {
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
pub struct ClientMovementPredictionError {
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl ClientMovementPredictionError {
    fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
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

impl std::error::Error for ClientMovementPredictionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Debug, Clone)]
struct MovementInput {
    movement: Vec2,
    view: Option<Vec2>,
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
            view: Some(Vec2::new(
                control.view_yaw().dequantize(),
                control.view_pitch().dequantize(),
            )),
        })
    }

    fn neutral_input(&self) -> MovementInput {
        MovementInput {
            movement: Vec2::ZERO,
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
                && predicted
                    .position_meters
                    .abs_diff_eq(authoritative.position_meters, POSITION_TOLERANCE_METERS)
                && predicted.velocity_meters_per_second.abs_diff_eq(
                    authoritative.velocity_meters_per_second,
                    VELOCITY_TOLERANCE_METERS_PER_SECOND,
                )
                && predicted
                    .orientation
                    .angle_between(authoritative.orientation)
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

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct PredictedPosition(Vec3);

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct PredictedVelocity(Vec3);

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct PredictedOrientation(Quat);

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct PredictedGrounded(u32);

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(C)]
struct PredictedInput {
    movement: Vec2,
    view_yaw_radians: f32,
    view_pitch_radians: f32,
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
        world
            .ecs_mut()
            .insert(entity, orientation, PredictedOrientation(Quat::IDENTITY))?;
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
            .map_or((0.0, 0.0, 0), |view| (view.x, view.y, 1));
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
        Vec2::new(input.view_yaw_radians, input.view_pitch_radians)
    };
    let yaw = Quat::from_rotation_y(view.x);
    let right = yaw * Vec3::X;
    let forward = yaw * Vec3::NEG_Z;
    velocity.0 = (right * input.movement.x + forward * input.movement.y)
        * PREDICTED_MOVEMENT_SPEED_METERS_PER_SECOND;
    position.0 += velocity.0 * PREDICTION_TICK_DELTA_SECONDS;
    orientation.0 = orientation_from_view(view);
}

fn orientation_from_view(view: Vec2) -> Quat {
    Quat::from_rotation_y(view.x) * Quat::from_rotation_x(view.y)
}

fn view_from_orientation(orientation: Quat) -> Vec2 {
    let (yaw, pitch, _roll) = orientation.to_euler(EulerRot::YXZ);
    Vec2::new(yaw.rem_euclid(std::f32::consts::TAU), pitch)
}

#[cfg(test)]
#[path = "../tests/unit/movement.rs"]
mod tests;
