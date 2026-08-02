use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use glam::{Quat, Vec3A};

use crate::character::{CharacterSettings, CharacterState, state_from_raw};
use crate::contact::{ContactEvent, event_from_raw};
use crate::error::{Error, UpdateError};
use crate::ffi::{self, Status};
use crate::ids::{BodyId, CharacterId, WorldKey};
use crate::raycast::{RayHit, hit_from_raw};
use crate::types::{BodySettings, StepDelta, validate_rotation, validate_vector};

const DEFAULT_MAX_BODIES: u32 = 1_024;
const DEFAULT_MAX_BODY_PAIRS: u32 = 1_024;
const DEFAULT_MAX_CONTACT_CONSTRAINTS: u32 = 1_024;

static NEXT_WORLD_KEY: AtomicU64 = AtomicU64::new(1);

/// Configuration used to create a [`World`].
#[derive(Debug, Clone, Copy)]
pub struct WorldBuilder {
    max_bodies: NonZeroU32,
    body_mutexes: u32,
    max_body_pairs: NonZeroU32,
    max_contact_constraints: NonZeroU32,
    worker_threads: NonZeroU32,
}

impl WorldBuilder {
    /// Construct the same small defaults used by Jolt's HelloWorld example.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_bodies: nonzero(DEFAULT_MAX_BODIES),
            body_mutexes: 0,
            max_body_pairs: nonzero(DEFAULT_MAX_BODY_PAIRS),
            max_contact_constraints: nonzero(DEFAULT_MAX_CONTACT_CONSTRAINTS),
            worker_threads: NonZeroU32::MIN,
        }
    }

    /// Set the maximum number of live rigid bodies.
    #[must_use]
    pub const fn max_bodies(mut self, max_bodies: NonZeroU32) -> Self {
        self.max_bodies = max_bodies;
        self
    }

    /// Set the number of mutexes protecting rigid bodies; zero selects Jolt's default.
    #[must_use]
    pub const fn body_mutexes(mut self, body_mutexes: u32) -> Self {
        self.body_mutexes = body_mutexes;
        self
    }

    /// Set the maximum number of overlapping body pairs queued by the broad phase.
    #[must_use]
    pub const fn max_body_pairs(mut self, max_body_pairs: NonZeroU32) -> Self {
        self.max_body_pairs = max_body_pairs;
        self
    }

    /// Set the maximum number of contact constraints.
    #[must_use]
    pub const fn max_contact_constraints(mut self, max_contact_constraints: NonZeroU32) -> Self {
        self.max_contact_constraints = max_contact_constraints;
        self
    }

    /// Set total physics concurrency, including the thread calling [`World::step`].
    #[must_use]
    pub const fn worker_threads(mut self, worker_threads: NonZeroU32) -> Self {
        self.worker_threads = worker_threads;
        self
    }

    /// Allocate and initialize the Jolt physics world.
    pub fn build(self) -> Result<World, Error> {
        let background_threads = self.worker_threads.get() - 1;
        let background_threads = i32::try_from(background_threads)
            .map_err(|_error| Error::WorkerCountTooLarge(self.worker_threads.get()))?;
        let pointer = ffi::create_world(ffi::WorldConfig {
            max_bodies: self.max_bodies.get(),
            body_mutexes: self.body_mutexes,
            max_body_pairs: self.max_body_pairs.get(),
            max_contact_constraints: self.max_contact_constraints.get(),
            worker_threads: background_threads,
        })
        .map_err(map_world_initialization)?;
        let key = WorldKey(NEXT_WORLD_KEY.fetch_add(1, Ordering::Relaxed));
        Ok(World {
            pointer,
            key,
            not_send_sync: PhantomData,
        })
    }
}

impl Default for WorldBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// An owning, safe Jolt physics world.
///
/// `World` is deliberately neither `Send` nor `Sync`. Jolt worker threads are
/// internal to [`World::step`] and complete before the method returns.
pub struct World {
    pointer: ffi::WorldPtr,
    key: WorldKey,
    not_send_sync: PhantomData<Rc<()>>,
}

impl World {
    /// Create a single-threaded world with small default capacities.
    pub fn new() -> Result<Self, Error> {
        WorldBuilder::new().build()
    }

    /// Start configuring a physics world.
    #[must_use]
    pub const fn builder() -> WorldBuilder {
        WorldBuilder::new()
    }

    /// Create and add a rigid body.
    pub fn create_body(&mut self, settings: BodySettings) -> Result<BodyId, Error> {
        if settings.shape.requires_static_body()
            && settings.motion_type != crate::MotionType::Static
        {
            return Err(Error::StaticShapeRequiresStaticBody);
        }
        let raw = ffi::create_body(self.pointer, &settings).map_err(map_status)?;
        Ok(BodyId {
            raw,
            world: self.key,
        })
    }

    /// Remove and destroy a rigid body.
    pub fn destroy_body(&mut self, body: BodyId) -> Result<(), Error> {
        self.validate_world(body)?;
        ffi::destroy_body(self.pointer, body.raw).map_err(map_status)
    }

    /// Test whether a body handle still names a live body.
    pub fn is_alive(&self, body: BodyId) -> Result<bool, Error> {
        self.validate_world(body)?;
        ffi::body_exists(self.pointer, body.raw).map_err(map_status)
    }

    /// Test whether a body is actively simulating.
    pub fn is_active(&self, body: BodyId) -> Result<bool, Error> {
        self.validate_world(body)?;
        ffi::body_is_active(self.pointer, body.raw).map_err(map_status)
    }

    /// Return a body's world-space position.
    pub fn position(&self, body: BodyId) -> Result<Vec3A, Error> {
        self.validate_world(body)?;
        ffi::body_position(self.pointer, body.raw).map_err(map_status)
    }

    /// Return a body's normalized world-space rotation.
    pub fn rotation(&self, body: BodyId) -> Result<Quat, Error> {
        self.validate_world(body)?;
        ffi::body_rotation(self.pointer, body.raw).map_err(map_status)
    }

    /// Set a body's finite, normalized world-space rotation and activate it.
    pub fn set_rotation(&mut self, body: BodyId, rotation: Quat) -> Result<(), Error> {
        self.validate_world(body)?;
        let rotation = validate_rotation(rotation)?;
        ffi::set_body_rotation(self.pointer, body.raw, rotation).map_err(map_status)
    }

    /// Return a body's world-space linear velocity.
    pub fn linear_velocity(&self, body: BodyId) -> Result<Vec3A, Error> {
        self.validate_world(body)?;
        ffi::body_linear_velocity(self.pointer, body.raw).map_err(map_status)
    }

    /// Set a body's finite world-space linear velocity.
    pub fn set_linear_velocity(&mut self, body: BodyId, velocity: Vec3A) -> Result<(), Error> {
        self.validate_world(body)?;
        let velocity = validate_vector(velocity)?;
        ffi::set_body_linear_velocity(self.pointer, body.raw, velocity).map_err(map_status)
    }

    /// Return a body's world-space angular velocity.
    pub fn angular_velocity(&self, body: BodyId) -> Result<Vec3A, Error> {
        self.validate_world(body)?;
        ffi::body_angular_velocity(self.pointer, body.raw).map_err(map_status)
    }

    /// Set a body's finite world-space angular velocity.
    pub fn set_angular_velocity(&mut self, body: BodyId, velocity: Vec3A) -> Result<(), Error> {
        self.validate_world(body)?;
        let velocity = validate_vector(velocity)?;
        ffi::set_body_angular_velocity(self.pointer, body.raw, velocity).map_err(map_status)
    }

    /// Accumulate a world-space force at a body's center of mass.
    pub fn add_force(&mut self, body: BodyId, force: Vec3A) -> Result<(), Error> {
        self.validate_world(body)?;
        let force = validate_vector(force)?;
        ffi::add_body_force(self.pointer, body.raw, force).map_err(map_status)
    }

    /// Accumulate a world-space force at a world-space point.
    pub fn add_force_at_point(
        &mut self,
        body: BodyId,
        force: Vec3A,
        point: Vec3A,
    ) -> Result<(), Error> {
        self.validate_world(body)?;
        let force = validate_vector(force)?;
        let point = validate_vector(point)?;
        ffi::add_body_force_at_point(self.pointer, body.raw, force, point).map_err(map_status)
    }

    /// Accumulate a world-space torque.
    pub fn add_torque(&mut self, body: BodyId, torque: Vec3A) -> Result<(), Error> {
        self.validate_world(body)?;
        let torque = validate_vector(torque)?;
        ffi::add_body_torque(self.pointer, body.raw, torque).map_err(map_status)
    }

    /// Apply an instantaneous world-space impulse at a body's center of mass.
    pub fn add_impulse(&mut self, body: BodyId, impulse: Vec3A) -> Result<(), Error> {
        self.validate_world(body)?;
        let impulse = validate_vector(impulse)?;
        ffi::add_body_impulse(self.pointer, body.raw, impulse).map_err(map_status)
    }

    /// Apply an instantaneous world-space impulse at a world-space point.
    pub fn add_impulse_at_point(
        &mut self,
        body: BodyId,
        impulse: Vec3A,
        point: Vec3A,
    ) -> Result<(), Error> {
        self.validate_world(body)?;
        let impulse = validate_vector(impulse)?;
        let point = validate_vector(point)?;
        ffi::add_body_impulse_at_point(self.pointer, body.raw, impulse, point).map_err(map_status)
    }

    /// Apply an instantaneous world-space angular impulse.
    pub fn add_angular_impulse(&mut self, body: BodyId, impulse: Vec3A) -> Result<(), Error> {
        self.validate_world(body)?;
        let impulse = validate_vector(impulse)?;
        ffi::add_body_angular_impulse(self.pointer, body.raw, impulse).map_err(map_status)
    }

    /// Create and add a rigid-body capsule character controller.
    pub fn create_character(&mut self, settings: CharacterSettings) -> Result<CharacterId, Error> {
        let raw = ffi::create_character(self.pointer, settings).map_err(map_status)?;
        Ok(CharacterId {
            raw,
            world: self.key,
        })
    }

    /// Remove and destroy a character controller and its rigid body.
    pub fn destroy_character(&mut self, character: CharacterId) -> Result<(), Error> {
        self.validate_character_world(character)?;
        ffi::destroy_character(self.pointer, character.raw).map_err(map_status)
    }

    /// Test whether a character handle still names a live controller.
    pub fn is_character_alive(&self, character: CharacterId) -> Result<bool, Error> {
        self.validate_character_world(character)?;
        ffi::body_exists(self.pointer, character.raw).map_err(map_status)
    }

    /// Set the finite world-space velocity requested for a character controller.
    pub fn set_character_linear_velocity(
        &mut self,
        character: CharacterId,
        velocity: Vec3A,
    ) -> Result<(), Error> {
        self.validate_character_world(character)?;
        let velocity = validate_vector(velocity)?;
        ffi::set_character_linear_velocity(self.pointer, character.raw, velocity)
            .map_err(map_status)
    }

    /// Refresh a character's support facts after the rigid-body world has stepped.
    pub fn refresh_character_ground_state(
        &mut self,
        character: CharacterId,
        max_separation_distance: f32,
    ) -> Result<(), Error> {
        self.validate_character_world(character)?;
        if !max_separation_distance.is_finite() || max_separation_distance < 0.0 {
            return Err(Error::InvalidCharacterGroundSeparation);
        }
        ffi::refresh_character_ground_state(self.pointer, character.raw, max_separation_distance)
            .map_err(map_status)
    }

    /// Capture a character's body and ground state.
    pub fn character_state(&self, character: CharacterId) -> Result<CharacterState, Error> {
        self.validate_character_world(character)?;
        let state = ffi::character_state(self.pointer, character.raw).map_err(map_status)?;
        state_from_raw(state, self.key)
    }

    /// Capture canonically ordered contact facts from the latest physics step.
    ///
    /// The snapshot is empty before the first step and replaced by every step.
    pub fn contact_events(&self) -> Result<Vec<ContactEvent>, Error> {
        ffi::contact_events(self.pointer)
            .map_err(map_status)?
            .into_iter()
            .map(|event| event_from_raw(event, self.key))
            .collect()
    }

    /// Return the closest rigid-body hit between two finite world-space points.
    pub fn cast_ray(&self, origin: Vec3A, end: Vec3A) -> Result<Option<RayHit>, Error> {
        let displacement = end - origin;
        if !origin.is_finite()
            || !end.is_finite()
            || !displacement.is_finite()
            || displacement.length_squared() <= 0.0
        {
            return Err(Error::InvalidRay);
        }
        ffi::cast_ray(self.pointer, origin, displacement)
            .map_err(map_status)
            .map(|hit| hit.map(|hit| hit_from_raw(hit, self.key)))
    }

    /// Optimize the broad phase after adding a large batch of bodies.
    pub fn optimize_broad_phase(&mut self) {
        ffi::optimize_broad_phase(self.pointer);
    }

    /// Advance the physics simulation.
    pub fn step(&mut self, delta: StepDelta, collision_steps: NonZeroU32) -> Result<(), Error> {
        let collision_steps = i32::try_from(collision_steps.get())
            .map_err(|_error| Error::CollisionStepCountTooLarge(collision_steps.get()))?;
        let update_errors =
            ffi::update(self.pointer, delta.as_seconds(), collision_steps).map_err(map_status)?;
        if update_errors == 0 {
            Ok(())
        } else {
            Err(UpdateError::new(update_errors).into())
        }
    }

    fn validate_world(&self, body: BodyId) -> Result<(), Error> {
        if body.world == self.key {
            Ok(())
        } else {
            Err(Error::WrongWorld)
        }
    }

    fn validate_character_world(&self, character: CharacterId) -> Result<(), Error> {
        if character.world == self.key {
            Ok(())
        } else {
            Err(Error::WrongCharacterWorld)
        }
    }
}

impl Drop for World {
    fn drop(&mut self) {
        ffi::destroy_world(self.pointer);
    }
}

const fn nonzero(value: u32) -> NonZeroU32 {
    match NonZeroU32::new(value) {
        Some(value) => value,
        None => unreachable!(),
    }
}

const fn map_world_initialization(status: Status) -> Error {
    match status {
        Status::InvalidArgument => Error::InvalidWorldConfiguration,
        Status::InitializationFailed => Error::WorldInitialization,
        Status::BodyCapacityExhausted
        | Status::BodyNotFound
        | Status::CharacterNotFound
        | Status::BodyOwnedByCharacter
        | Status::ShapeCreationFailed
        | Status::ContractViolation => Error::NativeContract,
    }
}

const fn map_status(status: Status) -> Error {
    match status {
        Status::BodyCapacityExhausted => Error::BodyCapacityExhausted,
        Status::BodyNotFound => Error::BodyNotFound,
        Status::CharacterNotFound => Error::CharacterNotFound,
        Status::BodyOwnedByCharacter => Error::BodyOwnedByCharacter,
        Status::ShapeCreationFailed => Error::ShapeCreationFailed,
        Status::InvalidArgument | Status::InitializationFailed | Status::ContractViolation => {
            Error::NativeContract
        }
    }
}
