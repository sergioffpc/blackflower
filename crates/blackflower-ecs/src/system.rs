use std::ffi::CString;
use std::marker::PhantomData;

use crate::error::{Error, SystemResult};
use crate::ffi::{self, ParallelContext, Projection, SystemContext};
use crate::ids::{EntityId, PhaseId, SystemId};
use crate::telemetry::{self, ResourceKind};
use crate::world::World;

/// Builder for a Flecs system and its typed Rust projection.
pub struct SystemBuilder<'world> {
    world: &'world mut World,
    name: String,
    name_c: CString,
    expression: String,
    expression_c: CString,
    phase: Option<PhaseId>,
}

impl<'world> SystemBuilder<'world> {
    pub(crate) fn new(
        world: &'world mut World,
        name: &str,
        expression: &str,
    ) -> Result<Self, Error> {
        if name.is_empty() {
            return Err(Error::InvalidName);
        }
        let name_c = CString::new(name).map_err(|_error| Error::InvalidName)?;
        let expression_c = CString::new(expression).map_err(|_error| Error::InvalidExpression)?;
        Ok(Self {
            world,
            name: name.to_owned(),
            name_c,
            expression: expression.to_owned(),
            expression_c,
            phase: None,
        })
    }

    /// Schedule the system in a built-in or custom phase.
    pub fn phase(mut self, phase: PhaseId) -> Result<Self, Error> {
        self.world.validate_phase(phase)?;
        self.phase = Some(phase);
        Ok(self)
    }

    /// Bind explicitly indexed DSL fields to checked Rust references.
    pub fn project<P: Projection>(
        self,
        projection: P,
    ) -> Result<ProjectedSystemBuilder<'world, P>, Error> {
        let specs = ffi::projection_specs(&projection, &|type_id| {
            self.world.resolve_component(type_id)
        })?;
        Ok(ProjectedSystemBuilder {
            world: self.world,
            name: self.name,
            name_c: self.name_c,
            expression: self.expression,
            expression_c: self.expression_c,
            phase: self.phase,
            specs,
            marker: PhantomData,
        })
    }
}

/// A system builder with a validated projection declaration.
pub struct ProjectedSystemBuilder<'world, P: Projection> {
    world: &'world mut World,
    name: String,
    name_c: CString,
    expression: String,
    expression_c: CString,
    phase: Option<PhaseId>,
    specs: Vec<ffi::FieldSpec>,
    marker: PhantomData<fn() -> P>,
}

impl<P: Projection> ProjectedSystemBuilder<'_, P> {
    /// Schedule the system in a built-in or custom phase.
    pub fn phase(mut self, phase: PhaseId) -> Result<Self, Error> {
        self.world.validate_phase(phase)?;
        self.phase = Some(phase);
        Ok(self)
    }

    /// Create a single-threaded system with deferred structural commands.
    pub fn each<F>(self, callback: F) -> Result<SystemId, Error>
    where
        P: 'static,
        F: for<'a> Fn(SystemContext<'a>, EntityId, P::Item<'a>) -> SystemResult + 'static,
    {
        let entity = self.world.create_named_system_entity(&self.name_c)?;
        let phase = self.phase.map(|value| value.raw);
        let definition = ffi::SystemDefinition {
            world: self.world.key,
            entity,
            expression: self.expression_c.as_c_str(),
            phase,
            specs: self.specs,
            failure: self.world.failure.clone(),
            name: self.name.clone(),
        };
        let Some(raw) = ffi::create_single_system::<P, F>(self.world.pointer, definition, callback)
        else {
            self.world.delete_raw_entity(entity);
            return Err(Error::SystemCreation(format!(
                "{}: {}",
                self.name, self.expression
            )));
        };
        telemetry::resource_registered(
            self.world.key,
            ResourceKind::System,
            &self.name,
            Some(false),
        );
        Ok(SystemId {
            raw,
            world: self.world.key,
        })
    }

    /// Create a multi-threaded system without structural access.
    pub fn parallel_each<F>(self, callback: F) -> Result<SystemId, Error>
    where
        P: 'static,
        F: for<'a> Fn(ParallelContext, EntityId, P::Item<'a>) -> SystemResult
            + Send
            + Sync
            + 'static,
    {
        let entity = self.world.create_named_system_entity(&self.name_c)?;
        let phase = self.phase.map(|value| value.raw);
        let definition = ffi::SystemDefinition {
            world: self.world.key,
            entity,
            expression: self.expression_c.as_c_str(),
            phase,
            specs: self.specs,
            failure: self.world.failure.clone(),
            name: self.name.clone(),
        };
        let Some(raw) =
            ffi::create_parallel_system::<P, F>(self.world.pointer, definition, callback)
        else {
            self.world.delete_raw_entity(entity);
            return Err(Error::SystemCreation(format!(
                "{}: {}",
                self.name, self.expression
            )));
        };
        telemetry::resource_registered(
            self.world.key,
            ResourceKind::System,
            &self.name,
            Some(true),
        );
        Ok(SystemId {
            raw,
            world: self.world.key,
        })
    }
}
