use std::any::TypeId;
use std::collections::BTreeMap;
use std::ffi::CString;
use std::marker::PhantomData;
use std::mem;
use std::num::NonZeroU32;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::component::{Component, Tag};
use crate::error::{Error, RunError};
use crate::ffi::{self, FailureState, WorldPtr};
use crate::ids::{
    BuiltinPhase, ComponentId, EntityId, PhaseId, PipelineId, TagId, TickDelta, WorldKey,
};
use crate::pipeline::PipelineBuilder;
use crate::query::QueryBuilder;
use crate::system::SystemBuilder;
use crate::telemetry::{self, ResourceKind, TickObservation, TickOutcome};

static NEXT_WORLD_KEY: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistrationKind {
    Component,
    Tag,
}

#[derive(Debug, Clone, Copy)]
struct Registration {
    raw: u64,
    name: &'static str,
    kind: RegistrationKind,
}

/// Configuration used to create a [`World`].
#[derive(Debug, Clone, Copy)]
pub struct WorldBuilder {
    worker_threads: NonZeroU32,
}

impl WorldBuilder {
    /// Create a builder configured for single-threaded progress.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            worker_threads: NonZeroU32::MIN,
        }
    }

    /// Configure Flecs' persistent worker pool.
    #[must_use]
    pub const fn worker_threads(mut self, worker_threads: NonZeroU32) -> Self {
        self.worker_threads = worker_threads;
        self
    }

    /// Allocate the Flecs world and start the configured workers.
    pub fn build(self) -> Result<World, Error> {
        let worker_count = i32::try_from(self.worker_threads.get())
            .map_err(|_error| Error::WorkerCountTooLarge(self.worker_threads.get()))?;
        let pointer = ffi::create_world().ok_or(Error::WorldInitialization)?;
        let workers_started = worker_count > 1;
        if workers_started {
            ffi::set_threads(pointer, worker_count);
        }

        let key = WorldKey(NEXT_WORLD_KEY.fetch_add(1, Ordering::Relaxed));
        let mut world = World {
            pointer,
            key,
            workers_started,
            registrations: BTreeMap::new(),
            failure: FailureState::new(key),
            telemetry: telemetry::State::default(),
            not_send_sync: PhantomData,
        };
        world.telemetry.prime(pointer);
        telemetry::world_created(key, self.worker_threads.get());
        Ok(world)
    }
}

impl Default for WorldBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// An owning, safe Flecs world.
///
/// `World` is deliberately neither `Send` nor `Sync`. Flecs' worker threads
/// remain internal to [`World::progress`] and [`World::run_pipeline`].
pub struct World {
    pub(crate) pointer: WorldPtr,
    pub(crate) key: WorldKey,
    workers_started: bool,
    registrations: BTreeMap<TypeId, Registration>,
    pub(crate) failure: Arc<FailureState>,
    telemetry: telemetry::State,
    not_send_sync: PhantomData<Rc<()>>,
}

impl World {
    /// Create a single-threaded world.
    pub fn new() -> Result<Self, Error> {
        WorldBuilder::new().build()
    }

    /// Start configuring a world.
    #[must_use]
    pub const fn builder() -> WorldBuilder {
        WorldBuilder::new()
    }

    /// Register a dense plain-data component.
    pub fn register_component<T: Component>(&mut self) -> Result<ComponentId<T>, Error> {
        self.register_component_with_storage::<T>(false)
    }

    /// Register a sparse plain-data component.
    pub fn register_sparse_component<T: Component>(&mut self) -> Result<ComponentId<T>, Error> {
        self.register_component_with_storage::<T>(true)
    }

    fn register_component_with_storage<T: Component>(
        &mut self,
        sparse: bool,
    ) -> Result<ComponentId<T>, Error> {
        if mem::size_of::<T>() == 0 {
            return Err(Error::InvalidComponentLayout(T::NAME));
        }
        self.ensure_type_available::<T>(T::NAME)?;
        let name = valid_name(T::NAME)?;
        let entity = self.create_named(&name, T::NAME)?;
        let Some(raw) = ffi::register_component::<T>(self.pointer, entity) else {
            ffi::delete_entity(self.pointer, entity);
            return Err(Error::ComponentRegistration(T::NAME));
        };
        if sparse {
            ffi::mark_sparse(self.pointer, raw);
        }
        self.registrations.insert(
            TypeId::of::<T>(),
            Registration {
                raw,
                name: T::NAME,
                kind: RegistrationKind::Component,
            },
        );
        telemetry::resource_registered(self.key, ResourceKind::Component, T::NAME, None);
        Ok(ComponentId {
            raw,
            world: self.key,
            marker: PhantomData,
        })
    }

    /// Register a zero-data tag.
    pub fn register_tag<T: Tag>(&mut self) -> Result<TagId<T>, Error> {
        self.ensure_type_available::<T>(T::NAME)?;
        let name = valid_name(T::NAME)?;
        let raw = self.create_named(&name, T::NAME)?;
        self.registrations.insert(
            TypeId::of::<T>(),
            Registration {
                raw,
                name: T::NAME,
                kind: RegistrationKind::Tag,
            },
        );
        telemetry::resource_registered(self.key, ResourceKind::Tag, T::NAME, None);
        Ok(TagId {
            raw,
            world: self.key,
            marker: PhantomData,
        })
    }

    /// Retrieve the handle of a previously registered component.
    pub fn component<T: Component>(&self) -> Result<ComponentId<T>, Error> {
        let registration = self.registration::<T>(RegistrationKind::Component, T::NAME)?;
        Ok(ComponentId {
            raw: registration.raw,
            world: self.key,
            marker: PhantomData,
        })
    }

    /// Retrieve the handle of a previously registered tag.
    pub fn tag<T: Tag>(&self) -> Result<TagId<T>, Error> {
        let registration = self.registration::<T>(RegistrationKind::Tag, T::NAME)?;
        Ok(TagId {
            raw: registration.raw,
            world: self.key,
            marker: PhantomData,
        })
    }

    /// Allow a component value to be inherited through `IsA`.
    pub fn make_inheritable<T: Component>(
        &mut self,
        component: ComponentId<T>,
    ) -> Result<(), Error> {
        self.validate_component(component)?;
        ffi::mark_inheritable(self.pointer, component.raw);
        Ok(())
    }

    /// Create an anonymous entity.
    pub fn spawn(&mut self) -> Result<EntityId, Error> {
        self.spawn_with_name(None)
    }

    /// Create a named entity.
    pub fn spawn_named(&mut self, name: &str) -> Result<EntityId, Error> {
        let name = valid_name(name)?;
        self.spawn_with_name(Some(&name))
    }

    fn spawn_with_name(&mut self, name: Option<&CString>) -> Result<EntityId, Error> {
        if let Some(name) = name {
            self.ensure_name_available(name)?;
        }
        let raw = ffi::create_entity(self.pointer, name.map(CString::as_c_str))
            .ok_or_else(|| Error::EntityCreation(name_text(name)))?;
        Ok(EntityId {
            raw,
            world: self.key,
        })
    }

    /// Destroy an entity.
    pub fn despawn(&mut self, entity: EntityId) -> Result<(), Error> {
        self.validate_entity(entity)?;
        ffi::delete_entity(self.pointer, entity.raw);
        Ok(())
    }

    /// Test whether an entity handle is currently alive.
    pub fn is_alive(&self, entity: EntityId) -> Result<bool, Error> {
        if entity.world != self.key {
            return Err(Error::WrongWorld);
        }
        Ok(ffi::is_alive(self.pointer, entity.raw))
    }

    /// Insert or replace a component.
    pub fn insert<T: Component>(
        &mut self,
        entity: EntityId,
        component: ComponentId<T>,
        value: T,
    ) -> Result<(), Error> {
        self.validate_entity(entity)?;
        self.validate_component(component)?;
        ffi::set_component(self.pointer, entity.raw, component.raw, &value);
        Ok(())
    }

    /// Remove a component.
    pub fn remove<T: Component>(
        &mut self,
        entity: EntityId,
        component: ComponentId<T>,
    ) -> Result<(), Error> {
        self.validate_entity(entity)?;
        self.validate_component(component)?;
        ffi::remove_id(self.pointer, entity.raw, component.raw);
        Ok(())
    }

    /// Test whether an entity has a component.
    pub fn has<T: Component>(
        &self,
        entity: EntityId,
        component: ComponentId<T>,
    ) -> Result<bool, Error> {
        self.validate_entity(entity)?;
        self.validate_component(component)?;
        Ok(ffi::has_id(self.pointer, entity.raw, component.raw))
    }

    /// Copy a component value out of the world.
    pub fn get<T: Component>(
        &self,
        entity: EntityId,
        component: ComponentId<T>,
    ) -> Result<Option<T>, Error> {
        self.validate_entity(entity)?;
        self.validate_component(component)?;
        Ok(ffi::get_component(self.pointer, entity.raw, component.raw))
    }

    /// Mutate a component in place and notify Flecs after the closure returns.
    pub fn with_mut<T, R>(
        &mut self,
        entity: EntityId,
        component: ComponentId<T>,
        callback: impl FnOnce(&mut T) -> R,
    ) -> Result<Option<R>, Error>
    where
        T: Component,
    {
        self.validate_entity(entity)?;
        self.validate_component(component)?;
        Ok(ffi::with_component_mut(
            self.pointer,
            entity.raw,
            component.raw,
            callback,
        ))
    }

    /// Add a tag to an entity.
    pub fn add_tag<T: Tag>(&mut self, entity: EntityId, tag: TagId<T>) -> Result<(), Error> {
        self.validate_entity(entity)?;
        self.validate_tag(tag)?;
        ffi::add_id(self.pointer, entity.raw, tag.raw);
        Ok(())
    }

    /// Remove a tag from an entity.
    pub fn remove_tag<T: Tag>(&mut self, entity: EntityId, tag: TagId<T>) -> Result<(), Error> {
        self.validate_entity(entity)?;
        self.validate_tag(tag)?;
        ffi::remove_id(self.pointer, entity.raw, tag.raw);
        Ok(())
    }

    /// Test whether an entity has a tag.
    pub fn has_tag<T: Tag>(&self, entity: EntityId, tag: TagId<T>) -> Result<bool, Error> {
        self.validate_entity(entity)?;
        self.validate_tag(tag)?;
        Ok(ffi::has_id(self.pointer, entity.raw, tag.raw))
    }

    /// Add a zero-data relationship pair.
    pub fn add_pair(
        &mut self,
        entity: EntityId,
        relation: EntityId,
        target: EntityId,
    ) -> Result<(), Error> {
        self.validate_entity(entity)?;
        self.validate_entity(relation)?;
        self.validate_entity(target)?;
        ffi::add_id(
            self.pointer,
            entity.raw,
            ffi::make_pair(relation.raw, target.raw),
        );
        Ok(())
    }

    /// Insert or replace a data-bearing relationship pair.
    pub fn insert_pair<T: Component>(
        &mut self,
        entity: EntityId,
        relation: ComponentId<T>,
        target: EntityId,
        value: T,
    ) -> Result<(), Error> {
        self.validate_entity(entity)?;
        self.validate_component(relation)?;
        self.validate_entity(target)?;
        ffi::set_component(
            self.pointer,
            entity.raw,
            ffi::make_pair(relation.raw, target.raw),
            &value,
        );
        Ok(())
    }

    /// Remove a relationship pair.
    pub fn remove_pair(
        &mut self,
        entity: EntityId,
        relation: EntityId,
        target: EntityId,
    ) -> Result<(), Error> {
        self.validate_entity(entity)?;
        self.validate_entity(relation)?;
        self.validate_entity(target)?;
        ffi::remove_id(
            self.pointer,
            entity.raw,
            ffi::make_pair(relation.raw, target.raw),
        );
        Ok(())
    }

    /// Test whether an entity has a relationship pair.
    pub fn has_pair(
        &self,
        entity: EntityId,
        relation: EntityId,
        target: EntityId,
    ) -> Result<bool, Error> {
        self.validate_entity(entity)?;
        self.validate_entity(relation)?;
        self.validate_entity(target)?;
        Ok(ffi::has_id(
            self.pointer,
            entity.raw,
            ffi::make_pair(relation.raw, target.raw),
        ))
    }

    /// Make an entity inherit components from a base entity.
    pub fn inherit(&mut self, entity: EntityId, base: EntityId) -> Result<(), Error> {
        self.validate_entity(entity)?;
        self.validate_entity(base)?;
        ffi::add_is_a(self.pointer, entity.raw, base.raw);
        Ok(())
    }

    /// Start building a temporary query from the full Flecs DSL.
    pub fn query(&mut self, expression: &str) -> Result<QueryBuilder<'_>, Error> {
        QueryBuilder::new(self, expression)
    }

    /// Start building a system from the full Flecs DSL.
    pub fn system(&mut self, name: &str, expression: &str) -> Result<SystemBuilder<'_>, Error> {
        SystemBuilder::new(self, name, expression)
    }

    /// Return one of Flecs' built-in phases.
    #[must_use]
    pub fn builtin_phase(&self, phase: BuiltinPhase) -> PhaseId {
        ffi::builtin_phase(self.key, phase)
    }

    /// Create a custom phase, optionally ordered after another phase.
    pub fn create_phase(&mut self, name: &str, after: Option<PhaseId>) -> Result<PhaseId, Error> {
        let after = match after {
            Some(phase) => {
                self.validate_phase(phase)?;
                Some(phase.raw)
            }
            None => None,
        };
        let name_c = valid_name(name)?;
        let raw = self.create_named(&name_c, name)?;
        ffi::configure_phase(self.pointer, raw, after);
        telemetry::resource_registered(self.key, ResourceKind::Phase, name, None);
        Ok(ffi::phase_id(self.key, raw))
    }

    /// Start building a pipeline from the full Flecs DSL.
    pub fn pipeline(&mut self, name: &str, expression: &str) -> Result<PipelineBuilder<'_>, Error> {
        PipelineBuilder::new(self, name, expression)
    }

    /// Select the pipeline used by [`World::progress`].
    pub fn set_pipeline(&mut self, pipeline: PipelineId) -> Result<(), Error> {
        self.validate_pipeline(pipeline)?;
        ffi::set_pipeline(self.pointer, pipeline.raw);
        Ok(())
    }

    /// Advance the selected pipeline by an explicit fixed delta.
    pub fn progress(&mut self, delta: TickDelta) -> Result<bool, RunError> {
        let observation = TickObservation::start("progress", self.key, delta, None);
        self.failure.clear();
        let should_continue = observation.in_scope(|| ffi::progress(self.pointer, delta));
        let result = if let Some(error) = self.failure.take() {
            Err(error)
        } else {
            Ok(should_continue)
        };
        let outcome = match &result {
            Err(_) => TickOutcome::Failed,
            Ok(true) => TickOutcome::Continued,
            Ok(false) => TickOutcome::Stopped,
        };
        observation.finish(outcome, &mut self.telemetry, self.pointer);
        result
    }

    /// Execute a specific pipeline once with an explicit fixed delta.
    pub fn run_pipeline(&mut self, pipeline: PipelineId, delta: TickDelta) -> Result<(), RunError> {
        if self.validate_pipeline(pipeline).is_err() {
            telemetry::rejected_pipeline(self.key, pipeline.world);
            return Err(RunError::WrongWorld);
        }
        let observation =
            TickObservation::start("run_pipeline", self.key, delta, Some(pipeline.raw));
        self.failure.clear();
        observation.in_scope(|| ffi::run_pipeline(self.pointer, pipeline.raw, delta));
        let result = if let Some(error) = self.failure.take() {
            Err(error)
        } else {
            Ok(())
        };
        let outcome = if result.is_ok() {
            TickOutcome::Completed
        } else {
            TickOutcome::Failed
        };
        observation.finish(outcome, &mut self.telemetry, self.pointer);
        result
    }

    pub(crate) fn resolve_component(&self, type_id: TypeId) -> Option<u64> {
        self.registrations
            .get(&type_id)
            .filter(|registration| registration.kind == RegistrationKind::Component)
            .map(|registration| registration.raw)
    }

    pub(crate) fn create_named_system_entity(&self, name: &CString) -> Result<u64, Error> {
        self.ensure_name_available(name)?;
        ffi::create_entity(self.pointer, Some(name.as_c_str()))
            .ok_or_else(|| Error::EntityCreation(name.to_string_lossy().into_owned()))
    }

    pub(crate) fn delete_raw_entity(&self, raw: u64) {
        ffi::delete_entity(self.pointer, raw);
    }

    pub(crate) fn validate_phase(&self, phase: PhaseId) -> Result<(), Error> {
        self.validate_world(phase.world)
    }

    pub(crate) fn validate_pipeline(&self, pipeline: PipelineId) -> Result<(), Error> {
        self.validate_world(pipeline.world)
    }

    fn create_named(&self, name: &CString, display_name: &str) -> Result<u64, Error> {
        self.ensure_name_available(name)?;
        ffi::create_entity(self.pointer, Some(name.as_c_str()))
            .ok_or_else(|| Error::EntityCreation(display_name.to_owned()))
    }

    fn ensure_name_available(&self, name: &CString) -> Result<(), Error> {
        if ffi::lookup(self.pointer, name.as_c_str()) == 0 {
            Ok(())
        } else {
            Err(Error::DuplicateName(name.to_string_lossy().into_owned()))
        }
    }

    fn ensure_type_available<T: 'static>(&self, name: &'static str) -> Result<(), Error> {
        if self.registrations.contains_key(&TypeId::of::<T>()) {
            Err(Error::DuplicateType(name))
        } else {
            Ok(())
        }
    }

    fn registration<T: 'static>(
        &self,
        kind: RegistrationKind,
        name: &'static str,
    ) -> Result<&Registration, Error> {
        self.registrations
            .get(&TypeId::of::<T>())
            .filter(|registration| registration.kind == kind && registration.name == name)
            .ok_or(Error::UnregisteredType(name))
    }

    fn validate_component<T: Component>(&self, component: ComponentId<T>) -> Result<(), Error> {
        self.validate_world(component.world)?;
        let registration = self.registration::<T>(RegistrationKind::Component, T::NAME)?;
        if registration.raw == component.raw {
            Ok(())
        } else {
            Err(Error::UnregisteredType(T::NAME))
        }
    }

    fn validate_tag<T: Tag>(&self, tag: TagId<T>) -> Result<(), Error> {
        self.validate_world(tag.world)?;
        let registration = self.registration::<T>(RegistrationKind::Tag, T::NAME)?;
        if registration.raw == tag.raw {
            Ok(())
        } else {
            Err(Error::UnregisteredType(T::NAME))
        }
    }

    fn validate_entity(&self, entity: EntityId) -> Result<(), Error> {
        self.validate_world(entity.world)?;
        if ffi::is_alive(self.pointer, entity.raw) {
            Ok(())
        } else {
            Err(Error::DeadEntity)
        }
    }

    fn validate_world(&self, world: WorldKey) -> Result<(), Error> {
        if world == self.key {
            Ok(())
        } else {
            Err(Error::WrongWorld)
        }
    }
}

impl Drop for World {
    fn drop(&mut self) {
        ffi::destroy_world(self.pointer, self.workers_started);
        let _telemetry_result = catch_unwind(AssertUnwindSafe(|| {
            self.telemetry.detach();
            telemetry::world_destroyed(self.key, self.workers_started);
        }));
    }
}

fn valid_name(name: &str) -> Result<CString, Error> {
    if name.is_empty() {
        return Err(Error::InvalidName);
    }
    CString::new(name).map_err(|_error| Error::InvalidName)
}

fn name_text(name: Option<&CString>) -> String {
    name.map_or_else(String::new, |value| value.to_string_lossy().into_owned())
}
