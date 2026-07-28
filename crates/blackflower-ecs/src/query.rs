use std::ffi::CString;
use std::marker::PhantomData;

use crate::error::Error;
use crate::ffi::{self, FieldSpec, Projection, QueryPtr};
use crate::ids::EntityId;
use crate::world::World;

/// A temporary query whose lifetime is tied to its world.
pub struct Query<'world, P: Projection> {
    world: &'world mut World,
    pointer: QueryPtr,
    specs: Vec<FieldSpec>,
    marker: PhantomData<fn() -> P>,
}

impl<P: Projection> Query<'_, P> {
    /// Visit every matching entity.
    pub fn each<F>(&mut self, mut callback: F) -> Result<(), Error>
    where
        F: for<'a> FnMut(EntityId, P::Item<'a>),
    {
        ffi::query_each::<P, _>(
            self.pointer,
            self.world.pointer,
            self.world.key,
            &self.specs,
            &mut |entity, item| {
                callback(entity, item);
                Ok(())
            },
        )
    }

    /// Visit every matching entity with a fallible callback.
    pub fn try_each<F>(&mut self, mut callback: F) -> Result<(), Error>
    where
        F: for<'a> FnMut(EntityId, P::Item<'a>) -> Result<(), Error>,
    {
        ffi::query_each::<P, _>(
            self.pointer,
            self.world.pointer,
            self.world.key,
            &self.specs,
            &mut callback,
        )
    }
}

impl<P: Projection> Drop for Query<'_, P> {
    fn drop(&mut self) {
        ffi::destroy_query(self.pointer);
    }
}

/// Builder that adds an explicit Rust projection to a Flecs DSL query.
pub struct QueryBuilder<'world> {
    world: &'world mut World,
    expression: String,
    expression_c: CString,
}

impl<'world> QueryBuilder<'world> {
    pub(crate) fn new(world: &'world mut World, expression: &str) -> Result<Self, Error> {
        let expression_c = CString::new(expression).map_err(|_error| Error::InvalidExpression)?;
        Ok(Self {
            world,
            expression: expression.to_owned(),
            expression_c,
        })
    }

    /// Bind explicitly indexed DSL fields to checked Rust references.
    pub fn project<P: Projection>(self, projection: P) -> Result<Query<'world, P>, Error> {
        let specs = ffi::projection_specs(&projection, &|type_id| {
            self.world.resolve_component(type_id)
        })?;
        let pointer = ffi::create_query(self.world.pointer, self.expression_c.as_c_str())
            .ok_or(Error::QueryCreation(self.expression))?;
        Ok(Query {
            world: self.world,
            pointer,
            specs,
            marker: PhantomData,
        })
    }
}
