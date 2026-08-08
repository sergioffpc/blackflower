use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::num::NonZeroU64;

use glam::{Quat, Vec2, Vec3};

use crate::{INPUT_GRACE_TICKS, SIMULATION_TICK_DELTA_SECONDS, SimulationTick};

/// Fixed maximum horizontal speed for the initial movement vertical slice.
pub const MOVEMENT_SPEED_METERS_PER_SECOND: f32 = 5.0;

/// Stable simulation-owned identity of one controllable actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActorId(NonZeroU64);

impl ActorId {
    /// Construct an actor identity from an already validated non-zero value.
    #[must_use]
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    /// Return the non-zero identity value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// One canonical four-tick movement control delivered through an in-memory boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementControl {
    actor: ActorId,
    input_sequence: u64,
    execute_tick: SimulationTick,
    movement: Vec2,
    view_yaw_radians: f32,
    view_pitch_radians: f32,
}

impl MovementControl {
    /// Validate one transport-independent movement control.
    pub fn new(
        actor: ActorId,
        input_sequence: u64,
        execute_tick: SimulationTick,
        movement: Vec2,
        view_yaw_radians: f32,
        view_pitch_radians: f32,
    ) -> Result<Self, MovementError> {
        if !movement.is_finite() || movement.length_squared() > 1.000_1 {
            return Err(MovementError::InvalidMovement);
        }
        if !view_yaw_radians.is_finite() || !(0.0..TAU).contains(&view_yaw_radians) {
            return Err(MovementError::InvalidYaw);
        }
        if !view_pitch_radians.is_finite()
            || !(-FRAC_PI_2..=FRAC_PI_2).contains(&view_pitch_radians)
        {
            return Err(MovementError::InvalidPitch);
        }
        Ok(Self {
            actor,
            input_sequence,
            execute_tick,
            movement,
            view_yaw_radians,
            view_pitch_radians,
        })
    }

    /// Return the controlled actor.
    #[must_use]
    pub const fn actor(self) -> ActorId {
        self.actor
    }

    /// Return the first tick covered by this control.
    #[must_use]
    pub const fn execute_tick(self) -> SimulationTick {
        self.execute_tick
    }
}

/// Sealed authoritative movement state for one actor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActorMovementState {
    /// Stable actor identity.
    pub actor: ActorId,
    /// Engine-space position in metres.
    pub position_meters: Vec3,
    /// Engine-space velocity in metres per second.
    pub velocity_meters_per_second: Vec3,
    /// Canonical world orientation.
    pub orientation: Quat,
    /// Whether the initial flat-ground controller is supported.
    pub grounded: bool,
    /// Latest input sequence applied by an authoritative tick.
    pub acknowledged_input_sequence: Option<u64>,
}

/// Immutable, actor-ordered movement frame sealed at one authoritative tick.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct MovementFrame {
    tick: SimulationTick,
    actors: Vec<ActorMovementState>,
}

impl MovementFrame {
    /// Return the authoritative tick represented by this frame.
    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        self.tick
    }

    /// Return actor state in stable identity order.
    #[must_use]
    pub fn actors(&self) -> &[ActorMovementState] {
        &self.actors
    }

    /// Resolve one actor by stable identity.
    #[must_use]
    pub fn actor(&self, id: ActorId) -> Option<&ActorMovementState> {
        self.actors
            .binary_search_by_key(&id, |state| state.actor)
            .ok()
            .map(|index| &self.actors[index])
    }
}

/// Invalid actor lifecycle or canonical in-memory movement control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MovementError {
    /// An actor identity is already present in the world.
    #[error("movement actor already exists")]
    DuplicateActor,
    /// An input targets an actor that does not exist.
    #[error("movement actor does not exist")]
    MissingActor,
    /// Local movement is non-finite or outside the normalized unit circle.
    #[error("movement vector is invalid")]
    InvalidMovement,
    /// Absolute yaw is not finite or outside the canonical full turn.
    #[error("view yaw is invalid")]
    InvalidYaw,
    /// Absolute pitch is not finite or outside the closed half-turn range.
    #[error("view pitch is invalid")]
    InvalidPitch,
    /// A different control was already scheduled for the same actor tick.
    #[error("actor tick already has a different movement control")]
    ConflictingControl,
    /// Expanding the four-tick control frame overflowed the tick domain.
    #[error("movement control tick overflow")]
    TickOverflow,
}

#[derive(Debug, Default)]
pub(crate) struct MovementRuntime {
    actors: BTreeMap<ActorId, ActorRuntime>,
}

impl MovementRuntime {
    pub(crate) fn spawn(&mut self, actor: ActorId) -> Result<(), MovementError> {
        if self.actors.contains_key(&actor) {
            return Err(MovementError::DuplicateActor);
        }
        self.actors.insert(actor, ActorRuntime::new(actor));
        Ok(())
    }

    pub(crate) fn despawn(&mut self, actor: ActorId) -> bool {
        self.actors.remove(&actor).is_some()
    }

    pub(crate) fn submit(
        &mut self,
        control: MovementControl,
        completed_tick: SimulationTick,
    ) -> Result<bool, MovementError> {
        let actor = self
            .actors
            .get_mut(&control.actor)
            .ok_or(MovementError::MissingActor)?;
        let mut accepted = Vec::with_capacity(4);
        for offset in 0_u64..4 {
            let tick = control
                .execute_tick
                .get()
                .checked_add(offset)
                .map(SimulationTick::new)
                .ok_or(MovementError::TickOverflow)?;
            if tick <= completed_tick {
                continue;
            }
            match actor.scheduled.get(&tick) {
                Some(existing) if existing == &control => {}
                Some(_existing) => return Err(MovementError::ConflictingControl),
                None => accepted.push(tick),
            }
        }
        for tick in &accepted {
            actor.scheduled.insert(*tick, control);
        }
        Ok(!accepted.is_empty())
    }

    pub(crate) fn capture(&mut self, tick: SimulationTick) {
        for actor in self.actors.values_mut() {
            if let Some(control) = actor.scheduled.remove(&tick) {
                actor.held = HeldControl {
                    control,
                    last_tick: tick,
                };
                actor.state.acknowledged_input_sequence = Some(control.input_sequence);
            }
            let age = tick.get().saturating_sub(actor.held.last_tick.get());
            actor.captured = if age <= INPUT_GRACE_TICKS {
                actor.held.control
            } else {
                MovementControl {
                    movement: Vec2::ZERO,
                    ..actor.held.control
                }
            };
        }
    }

    pub(crate) fn derive(&mut self) {
        for actor in self.actors.values_mut() {
            let control = actor.captured;
            let yaw = Quat::from_rotation_y(control.view_yaw_radians);
            let right = yaw * Vec3::X;
            let forward = yaw * Vec3::NEG_Z;
            actor.state.velocity_meters_per_second = (right * control.movement.x
                + forward * control.movement.y)
                * MOVEMENT_SPEED_METERS_PER_SECOND;
            actor.state.orientation =
                orientation(control.view_yaw_radians, control.view_pitch_radians);
        }
    }

    pub(crate) fn advance(&mut self) {
        for actor in self.actors.values_mut() {
            actor.state.position_meters +=
                actor.state.velocity_meters_per_second * SIMULATION_TICK_DELTA_SECONDS;
        }
    }

    pub(crate) fn frame(&self, tick: SimulationTick) -> MovementFrame {
        MovementFrame {
            tick,
            actors: self.actors.values().map(|actor| actor.state).collect(),
        }
    }
}

#[derive(Debug)]
struct ActorRuntime {
    state: ActorMovementState,
    scheduled: BTreeMap<SimulationTick, MovementControl>,
    held: HeldControl,
    captured: MovementControl,
}

impl ActorRuntime {
    fn new(actor: ActorId) -> Self {
        let control = MovementControl {
            actor,
            input_sequence: 0,
            execute_tick: SimulationTick::ZERO,
            movement: Vec2::ZERO,
            view_yaw_radians: 0.0,
            view_pitch_radians: 0.0,
        };
        Self {
            state: ActorMovementState {
                actor,
                position_meters: Vec3::ZERO,
                velocity_meters_per_second: Vec3::ZERO,
                orientation: Quat::IDENTITY,
                grounded: true,
                acknowledged_input_sequence: None,
            },
            scheduled: BTreeMap::new(),
            held: HeldControl {
                control,
                last_tick: SimulationTick::ZERO,
            },
            captured: control,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct HeldControl {
    control: MovementControl,
    last_tick: SimulationTick,
}

fn orientation(yaw: f32, pitch: f32) -> Quat {
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)
}
