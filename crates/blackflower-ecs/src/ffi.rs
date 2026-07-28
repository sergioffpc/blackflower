#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw Flecs calls and pointer materialization are isolated in this private module"
)]
#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block,
    reason = "all unsafe operations are confined to the reviewed Flecs FFI boundary"
)]
#![allow(
    private_interfaces,
    reason = "sealed projection implementation details are intentionally not nameable by users"
)]

use std::any::{Any, TypeId};
use std::ffi::{CStr, c_void};
use std::marker::PhantomData;
use std::mem;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::component::Component;
use crate::error::{Error, ProjectionError, RunError, SystemResult};
use crate::ids::{BuiltinPhase, ComponentId, EntityId, PhaseId, PipelineId, TickDelta, WorldKey};
use crate::telemetry::{self, CallbackFailureKind};

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the Flecs v4.1.6 C API"
)]
#[allow(
    clippy::all,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::ptr_offset_with_cast,
    clippy::upper_case_acronyms,
    clippy::useless_transmute,
    reason = "bindgen-generated code mirrors C layouts and is not maintained by hand"
)]
pub(crate) mod raw {
    include!(concat!(env!("OUT_DIR"), "/flecs_bindings.rs"));
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WorldPtr(NonNull<raw::ecs_world_t>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct QueryPtr(NonNull<raw::ecs_query_t>);

#[derive(Debug, Clone, Copy)]
struct IterPtr(NonNull<raw::ecs_iter_t>);

pub(crate) fn create_world() -> Option<WorldPtr> {
    NonNull::new(unsafe { raw::ecs_init() }).map(WorldPtr)
}

pub(crate) fn destroy_world(world: WorldPtr, workers_started: bool) {
    if workers_started {
        unsafe { raw::ecs_set_threads(world.0.as_ptr(), 0) };
    }
    let _status = unsafe { raw::ecs_fini(world.0.as_ptr()) };
}

pub(crate) fn set_threads(world: WorldPtr, count: i32) {
    unsafe { raw::ecs_set_threads(world.0.as_ptr(), count) };
}

pub(crate) fn create_entity(world: WorldPtr, name: Option<&CStr>) -> Option<u64> {
    let mut descriptor = raw::ecs_entity_desc_t::default();
    if let Some(name) = name {
        descriptor.name = name.as_ptr();
    }
    NonNullId::new(unsafe { raw::ecs_entity_init(world.0.as_ptr(), &raw const descriptor) })
        .map(NonNullId::get)
}

pub(crate) fn register_component<T: Component>(world: WorldPtr, entity: u64) -> Option<u64> {
    let mut descriptor = raw::ecs_component_desc_t {
        entity,
        ..raw::ecs_component_desc_t::default()
    };
    descriptor.type_.size = i32::try_from(mem::size_of::<T>()).ok()?;
    descriptor.type_.alignment = i32::try_from(mem::align_of::<T>()).ok()?;

    NonNullId::new(unsafe { raw::ecs_component_init(world.0.as_ptr(), &raw const descriptor) })
        .map(NonNullId::get)
}

pub(crate) fn is_alive(world: WorldPtr, entity: u64) -> bool {
    unsafe { raw::ecs_is_alive(world.0.as_ptr(), entity) }
}

pub(crate) fn lookup(world: WorldPtr, name: &CStr) -> u64 {
    unsafe { raw::ecs_lookup(world.0.as_ptr(), name.as_ptr()) }
}

pub(crate) fn delete_entity(world: WorldPtr, entity: u64) {
    unsafe { raw::ecs_delete(world.0.as_ptr(), entity) };
}

pub(crate) fn add_id(world: WorldPtr, entity: u64, id: u64) {
    unsafe { raw::ecs_add_id(world.0.as_ptr(), entity, id) };
}

pub(crate) fn remove_id(world: WorldPtr, entity: u64, id: u64) {
    unsafe { raw::ecs_remove_id(world.0.as_ptr(), entity, id) };
}

pub(crate) fn has_id(world: WorldPtr, entity: u64, id: u64) -> bool {
    unsafe { raw::ecs_has_id(world.0.as_ptr(), entity, id) }
}

pub(crate) fn make_pair(first: u64, second: u64) -> u64 {
    unsafe { raw::ecs_make_pair(first, second) }
}

pub(crate) fn mark_sparse(world: WorldPtr, component: u64) {
    let sparse = unsafe { raw::EcsSparse };
    add_id(world, component, sparse);
}

pub(crate) fn mark_inheritable(world: WorldPtr, component: u64) {
    let on_instantiate = unsafe { raw::EcsOnInstantiate };
    let inherit = unsafe { raw::EcsInherit };
    add_id(world, component, make_pair(on_instantiate, inherit));
}

pub(crate) fn add_is_a(world: WorldPtr, entity: u64, base: u64) {
    let is_a = unsafe { raw::EcsIsA };
    add_id(world, entity, make_pair(is_a, base));
}

pub(crate) fn set_component<T: Component>(world: WorldPtr, entity: u64, id: u64, value: &T) {
    unsafe {
        raw::ecs_set_id(
            world.0.as_ptr(),
            entity,
            id,
            mem::size_of::<T>(),
            (value as *const T).cast(),
        );
    }
}

pub(crate) fn get_component<T: Component>(world: WorldPtr, entity: u64, id: u64) -> Option<T> {
    let pointer = unsafe { raw::ecs_get_id(world.0.as_ptr(), entity, id) }.cast::<T>();
    if pointer.is_null() {
        None
    } else {
        Some(unsafe { pointer.read() })
    }
}

pub(crate) fn with_component_mut<T, R>(
    world: WorldPtr,
    entity: u64,
    id: u64,
    callback: impl FnOnce(&mut T) -> R,
) -> Option<R>
where
    T: Component,
{
    let pointer = unsafe { raw::ecs_get_mut_id(world.0.as_ptr(), entity, id) }.cast::<T>();
    if pointer.is_null() {
        return None;
    }

    let result = catch_unwind(AssertUnwindSafe(|| callback(unsafe { &mut *pointer })));
    unsafe { raw::ecs_modified_id(world.0.as_ptr(), entity, id) };
    match result {
        Ok(value) => Some(value),
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

pub(crate) fn builtin_phase(world: WorldKey, phase: BuiltinPhase) -> PhaseId {
    let raw = unsafe {
        match phase {
            BuiltinPhase::OnStart => raw::EcsOnStart,
            BuiltinPhase::PreFrame => raw::EcsPreFrame,
            BuiltinPhase::OnLoad => raw::EcsOnLoad,
            BuiltinPhase::PostLoad => raw::EcsPostLoad,
            BuiltinPhase::PreUpdate => raw::EcsPreUpdate,
            BuiltinPhase::OnUpdate => raw::EcsOnUpdate,
            BuiltinPhase::OnValidate => raw::EcsOnValidate,
            BuiltinPhase::PostUpdate => raw::EcsPostUpdate,
            BuiltinPhase::PreStore => raw::EcsPreStore,
            BuiltinPhase::OnStore => raw::EcsOnStore,
            BuiltinPhase::PostFrame => raw::EcsPostFrame,
        }
    };
    PhaseId { raw, world }
}

pub(crate) fn configure_phase(world: WorldPtr, entity: u64, after: Option<u64>) {
    unsafe { raw::ecs_add_id(world.0.as_ptr(), entity, raw::EcsPhase) };
    if let Some(after) = after {
        let depends_on = unsafe { raw::EcsDependsOn };
        let pair = unsafe { raw::ecs_make_pair(depends_on, after) };
        unsafe { raw::ecs_add_id(world.0.as_ptr(), entity, pair) };
    }
}

pub(crate) fn create_query(world: WorldPtr, expression: &CStr) -> Option<QueryPtr> {
    let descriptor = raw::ecs_query_desc_t {
        expr: expression.as_ptr(),
        ..raw::ecs_query_desc_t::default()
    };
    NonNull::new(unsafe { raw::ecs_query_init(world.0.as_ptr(), &raw const descriptor) })
        .map(QueryPtr)
}

pub(crate) fn destroy_query(query: QueryPtr) {
    unsafe { raw::ecs_query_fini(query.0.as_ptr()) };
}

pub(crate) fn create_pipeline(world: WorldPtr, entity: u64, expression: &CStr) -> Option<u64> {
    let descriptor = raw::ecs_pipeline_desc_t {
        entity,
        query: raw::ecs_query_desc_t {
            expr: expression.as_ptr(),
            ..raw::ecs_query_desc_t::default()
        },
    };
    NonNullId::new(unsafe { raw::ecs_pipeline_init(world.0.as_ptr(), &raw const descriptor) })
        .map(NonNullId::get)
}

pub(crate) fn set_pipeline(world: WorldPtr, pipeline: u64) {
    unsafe { raw::ecs_set_pipeline(world.0.as_ptr(), pipeline) };
}

pub(crate) fn progress(world: WorldPtr, delta: TickDelta) -> bool {
    unsafe { raw::ecs_progress(world.0.as_ptr(), delta.seconds()) }
}

pub(crate) fn run_pipeline(world: WorldPtr, pipeline: u64, delta: TickDelta) {
    unsafe { raw::ecs_run_pipeline(world.0.as_ptr(), pipeline, delta.seconds()) };
}

#[cfg(feature = "metrics")]
pub(crate) struct WorldStatsState {
    stats: Box<raw::ecs_world_stats_t>,
}

#[cfg(feature = "metrics")]
impl Default for WorldStatsState {
    fn default() -> Self {
        Self {
            stats: Box::new(raw::ecs_world_stats_t::default()),
        }
    }
}

#[cfg(feature = "metrics")]
#[derive(Debug, Clone, Copy)]
pub(crate) struct WorldStatsSample {
    pub(crate) entities: f64,
    pub(crate) tables: f64,
    pub(crate) queries: f64,
    pub(crate) systems: f64,
    pub(crate) allocations_outstanding: f64,
    pub(crate) systems_ran: f64,
    pub(crate) merges: f64,
    pub(crate) rematches: f64,
    pub(crate) pipeline_rebuilds: f64,
    pub(crate) frame_time_seconds: f64,
    pub(crate) system_time_seconds: f64,
    pub(crate) merge_time_seconds: f64,
    pub(crate) rematch_time_seconds: f64,
    pub(crate) command_adds: f64,
    pub(crate) command_removes: f64,
    pub(crate) command_deletes: f64,
    pub(crate) command_clears: f64,
    pub(crate) command_sets: f64,
    pub(crate) command_ensures: f64,
    pub(crate) command_modifications: f64,
    pub(crate) command_other: f64,
    pub(crate) command_discards: f64,
}

#[cfg(feature = "metrics")]
pub(crate) fn sample_world_stats(world: WorldPtr, state: &mut WorldStatsState) -> WorldStatsSample {
    unsafe { raw::ecs_world_stats_get(world.0.as_ptr(), state.stats.as_mut()) };
    let index = usize::try_from(state.stats.t)
        .ok()
        .filter(|index| *index < raw::ECS_STAT_WINDOW as usize)
        .unwrap_or_default();
    WorldStatsSample {
        entities: metric_gauge(&state.stats.entities.count, index),
        tables: metric_gauge(&state.stats.tables.count, index),
        queries: metric_gauge(&state.stats.queries.query_count, index),
        systems: metric_gauge(&state.stats.queries.system_count, index),
        allocations_outstanding: metric_gauge(&state.stats.memory.outstanding_alloc_count, index),
        systems_ran: metric_counter_rate(&state.stats.frame.systems_ran, index),
        merges: metric_counter_rate(&state.stats.frame.merge_count, index),
        rematches: metric_counter_rate(&state.stats.frame.rematch_count, index),
        pipeline_rebuilds: metric_counter_rate(&state.stats.frame.pipeline_build_count, index),
        frame_time_seconds: metric_counter_rate(&state.stats.performance.frame_time, index),
        system_time_seconds: metric_counter_rate(&state.stats.performance.system_time, index),
        merge_time_seconds: metric_counter_rate(&state.stats.performance.merge_time, index),
        rematch_time_seconds: metric_counter_rate(&state.stats.performance.rematch_time, index),
        command_adds: metric_counter_rate(&state.stats.commands.add_count, index),
        command_removes: metric_counter_rate(&state.stats.commands.remove_count, index),
        command_deletes: metric_counter_rate(&state.stats.commands.delete_count, index),
        command_clears: metric_counter_rate(&state.stats.commands.clear_count, index),
        command_sets: metric_counter_rate(&state.stats.commands.set_count, index),
        command_ensures: metric_counter_rate(&state.stats.commands.ensure_count, index),
        command_modifications: metric_counter_rate(&state.stats.commands.modified_count, index),
        command_other: metric_counter_rate(&state.stats.commands.other_count, index),
        command_discards: metric_counter_rate(&state.stats.commands.discard_count, index),
    }
}

#[cfg(feature = "metrics")]
fn metric_gauge(metric: &raw::ecs_metric_t, index: usize) -> f64 {
    f64::from(unsafe { metric.gauge.avg[index] })
}

#[cfg(feature = "metrics")]
fn metric_counter_rate(metric: &raw::ecs_metric_t, index: usize) -> f64 {
    f64::from(unsafe { metric.counter.rate.avg[index] })
}

#[derive(Debug, Clone, Copy)]
struct NonNullId(u64);

impl NonNullId {
    fn new(value: u64) -> Option<Self> {
        (value != 0).then_some(Self(value))
    }

    fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Component,
    Pair,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FieldSpec {
    index: i8,
    component: u64,
    size: usize,
    alignment: usize,
    access: Access,
    shape: Shape,
    optional: bool,
}

#[derive(Debug)]
struct ResolvedField {
    spec: FieldSpec,
    pointer: Option<NonNull<u8>>,
    pair: Option<(u64, u64)>,
}

/// Read projection for an ordinary component field.
#[derive(Debug, Clone, Copy)]
pub struct Read<T> {
    index: i8,
    marker: PhantomData<fn() -> T>,
}

impl<T> Read<T> {
    #[must_use]
    pub const fn field(index: i8) -> Self {
        Self {
            index,
            marker: PhantomData,
        }
    }
}

/// Mutable projection for an ordinary, self-owned component field.
#[derive(Debug, Clone, Copy)]
pub struct Write<T> {
    index: i8,
    marker: PhantomData<fn() -> T>,
}

impl<T> Write<T> {
    #[must_use]
    pub const fn field(index: i8) -> Self {
        Self {
            index,
            marker: PhantomData,
        }
    }
}

/// Read projection for a data-bearing pair field.
#[derive(Debug, Clone, Copy)]
pub struct PairRead<T> {
    index: i8,
    marker: PhantomData<fn() -> T>,
}

impl<T> PairRead<T> {
    #[must_use]
    pub const fn field(index: i8) -> Self {
        Self {
            index,
            marker: PhantomData,
        }
    }
}

/// Mutable projection for a self-owned, data-bearing pair field.
#[derive(Debug, Clone, Copy)]
pub struct PairWrite<T> {
    index: i8,
    marker: PhantomData<fn() -> T>,
}

impl<T> PairWrite<T> {
    #[must_use]
    pub const fn field(index: i8) -> Self {
        Self {
            index,
            marker: PhantomData,
        }
    }
}

/// Mark a projected field as optional.
#[derive(Debug, Clone, Copy)]
pub struct Optional<F>(F);

impl<F> Optional<F> {
    #[must_use]
    pub const fn new(field: F) -> Self {
        Self(field)
    }
}

/// Read access to the value and matched IDs of a pair field.
#[derive(Debug)]
pub struct PairRef<'a, T> {
    relation: EntityId,
    target: EntityId,
    value: &'a T,
}

impl<T> PairRef<'_, T> {
    #[must_use]
    pub fn relation(&self) -> EntityId {
        self.relation
    }

    #[must_use]
    pub fn target(&self) -> EntityId {
        self.target
    }

    #[must_use]
    pub fn value(&self) -> &T {
        self.value
    }
}

/// Mutable access to the value and matched IDs of a pair field.
#[derive(Debug)]
pub struct PairMut<'a, T> {
    relation: EntityId,
    target: EntityId,
    value: &'a mut T,
}

impl<T> PairMut<'_, T> {
    #[must_use]
    pub fn relation(&self) -> EntityId {
        self.relation
    }

    #[must_use]
    pub fn target(&self) -> EntityId {
        self.target
    }

    #[must_use]
    pub fn value(&self) -> &T {
        self.value
    }

    #[must_use]
    pub fn value_mut(&mut self) -> &mut T {
        self.value
    }
}

pub trait Projection: sealed::Sealed {
    #[doc(hidden)]
    type Item<'a>
    where
        Self: 'a;

    #[doc(hidden)]
    fn specs(&self, resolve: &dyn Fn(TypeId) -> Option<u64>) -> Result<Vec<FieldSpec>, Error>;

    #[doc(hidden)]
    unsafe fn materialize<'a>(resolved: &'a [ResolvedField], world: WorldKey) -> Self::Item<'a>;
}

mod sealed {
    pub trait Sealed {}
}

#[doc(hidden)]
pub trait FieldProjection: sealed::Sealed {
    type Value<'a>
    where
        Self: 'a;

    fn spec(
        &self,
        resolve: &dyn Fn(TypeId) -> Option<u64>,
        optional: bool,
    ) -> Result<FieldSpec, Error>;

    unsafe fn value<'a>(resolved: &'a ResolvedField, world: WorldKey) -> Self::Value<'a>;
}

fn field_spec<T: Component>(
    index: i8,
    access: Access,
    shape: Shape,
    optional: bool,
    resolve: &dyn Fn(TypeId) -> Option<u64>,
) -> Result<FieldSpec, Error> {
    let component = resolve(TypeId::of::<T>()).ok_or(Error::UnregisteredType(T::NAME))?;
    Ok(FieldSpec {
        index,
        component,
        size: mem::size_of::<T>(),
        alignment: mem::align_of::<T>(),
        access,
        shape,
        optional,
    })
}

macro_rules! impl_plain_field {
    ($marker:ident, $access:expr, $value:ty, $materialize:expr) => {
        impl<T: Component> sealed::Sealed for $marker<T> {}

        impl<T: Component> FieldProjection for $marker<T> {
            type Value<'a>
                = $value
            where
                Self: 'a;

            fn spec(
                &self,
                resolve: &dyn Fn(TypeId) -> Option<u64>,
                optional: bool,
            ) -> Result<FieldSpec, Error> {
                field_spec::<T>(self.index, $access, Shape::Component, optional, resolve)
            }

            unsafe fn value<'a>(resolved: &'a ResolvedField, _world: WorldKey) -> Self::Value<'a> {
                $materialize(resolved)
            }
        }
    };
}

impl_plain_field!(Read, Access::Read, &'a T, |resolved: &'a ResolvedField| {
    unsafe { &*required_pointer::<T>(resolved) }
});
impl_plain_field!(
    Write,
    Access::Write,
    &'a mut T,
    |resolved: &'a ResolvedField| { unsafe { &mut *required_pointer::<T>(resolved) } }
);

impl<T: Component> sealed::Sealed for PairRead<T> {}

impl<T: Component> FieldProjection for PairRead<T> {
    type Value<'a>
        = PairRef<'a, T>
    where
        Self: 'a;

    fn spec(
        &self,
        resolve: &dyn Fn(TypeId) -> Option<u64>,
        optional: bool,
    ) -> Result<FieldSpec, Error> {
        field_spec::<T>(self.index, Access::Read, Shape::Pair, optional, resolve)
    }

    unsafe fn value<'a>(resolved: &'a ResolvedField, world: WorldKey) -> Self::Value<'a> {
        let (relation, target) = unsafe { required_pair(resolved) };
        PairRef {
            relation: EntityId {
                raw: relation,
                world,
            },
            target: EntityId { raw: target, world },
            value: unsafe { &*required_pointer::<T>(resolved) },
        }
    }
}

impl<T: Component> sealed::Sealed for PairWrite<T> {}

impl<T: Component> FieldProjection for PairWrite<T> {
    type Value<'a>
        = PairMut<'a, T>
    where
        Self: 'a;

    fn spec(
        &self,
        resolve: &dyn Fn(TypeId) -> Option<u64>,
        optional: bool,
    ) -> Result<FieldSpec, Error> {
        field_spec::<T>(self.index, Access::Write, Shape::Pair, optional, resolve)
    }

    unsafe fn value<'a>(resolved: &'a ResolvedField, world: WorldKey) -> Self::Value<'a> {
        let (relation, target) = unsafe { required_pair(resolved) };
        PairMut {
            relation: EntityId {
                raw: relation,
                world,
            },
            target: EntityId { raw: target, world },
            value: unsafe { &mut *required_pointer::<T>(resolved) },
        }
    }
}

unsafe fn required_pointer<T>(resolved: &ResolvedField) -> *mut T {
    let Some(pointer) = resolved.pointer else {
        unsafe { std::hint::unreachable_unchecked() }
    };
    pointer.as_ptr().cast::<T>()
}

unsafe fn required_pair(resolved: &ResolvedField) -> (u64, u64) {
    let Some(pair) = resolved.pair else {
        unsafe { std::hint::unreachable_unchecked() }
    };
    pair
}

impl<F: FieldProjection> sealed::Sealed for Optional<F> {}

impl<F: FieldProjection> FieldProjection for Optional<F> {
    type Value<'a>
        = Option<F::Value<'a>>
    where
        Self: 'a;

    fn spec(
        &self,
        resolve: &dyn Fn(TypeId) -> Option<u64>,
        _optional: bool,
    ) -> Result<FieldSpec, Error> {
        self.0.spec(resolve, true)
    }

    unsafe fn value<'a>(resolved: &'a ResolvedField, world: WorldKey) -> Self::Value<'a> {
        resolved
            .pointer
            .map(|_| unsafe { F::value(resolved, world) })
    }
}

impl<F: FieldProjection> Projection for F {
    type Item<'a>
        = F::Value<'a>
    where
        Self: 'a;

    fn specs(&self, resolve: &dyn Fn(TypeId) -> Option<u64>) -> Result<Vec<FieldSpec>, Error> {
        Ok(vec![self.spec(resolve, false)?])
    }

    unsafe fn materialize<'a>(resolved: &'a [ResolvedField], world: WorldKey) -> Self::Item<'a> {
        F::value(&resolved[0], world)
    }
}

macro_rules! impl_projection_tuple {
    ($(($type:ident, $index:tt)),+ $(,)?) => {
        impl<$($type: FieldProjection),+> sealed::Sealed for ($($type,)+) {}

        impl<$($type: FieldProjection),+> Projection for ($($type,)+) {
            type Item<'a> = ($($type::Value<'a>,)+) where Self: 'a;

            fn specs(
                &self,
                resolve: &dyn Fn(TypeId) -> Option<u64>,
            ) -> Result<Vec<FieldSpec>, Error> {
                Ok(vec![$(self.$index.spec(resolve, false)?,)+])
            }

            unsafe fn materialize<'a>(
                resolved: &'a [ResolvedField],
                world: WorldKey,
            ) -> Self::Item<'a> {
                ($($type::value(&resolved[$index], world),)+)
            }
        }
    };
}

impl_projection_tuple!((A, 0), (B, 1));
impl_projection_tuple!((A, 0), (B, 1), (C, 2));
impl_projection_tuple!((A, 0), (B, 1), (C, 2), (D, 3));
impl_projection_tuple!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4));
impl_projection_tuple!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4), (F, 5));
impl_projection_tuple!((A, 0), (B, 1), (C, 2), (D, 3), (E, 4), (F, 5), (G, 6));
impl_projection_tuple!(
    (A, 0),
    (B, 1),
    (C, 2),
    (D, 3),
    (E, 4),
    (F, 5),
    (G, 6),
    (H, 7)
);

fn resolve_fields(
    iterator: IterPtr,
    row: i32,
    specs: &[FieldSpec],
) -> Result<Vec<ResolvedField>, Error> {
    let iter = unsafe { iterator.0.as_ref() };
    let mut resolved = Vec::with_capacity(specs.len());
    for spec in specs {
        resolved.push(resolve_field(iterator, iter, row, *spec)?);
    }
    validate_aliases(&resolved)?;
    Ok(resolved)
}

fn resolve_field(
    iterator: IterPtr,
    iter: &raw::ecs_iter_t,
    row: i32,
    spec: FieldSpec,
) -> Result<ResolvedField, Error> {
    if spec.index < 0 || spec.index >= iter.field_count {
        return Err(Error::Projection(ProjectionError::FieldOutOfRange(
            spec.index,
        )));
    }
    if unsafe { raw::ecs_field_is_set(iterator.0.as_ptr(), spec.index) } {
        resolve_present_field(iterator, iter, row, spec)
    } else if spec.optional {
        Ok(ResolvedField {
            spec,
            pointer: None,
            pair: None,
        })
    } else {
        Err(Error::Projection(ProjectionError::RequiredFieldMissing(
            spec.index,
        )))
    }
}

fn resolve_present_field(
    iterator: IterPtr,
    iter: &raw::ecs_iter_t,
    row: i32,
    spec: FieldSpec,
) -> Result<ResolvedField, Error> {
    validate_access(iterator, spec)?;
    let field_id = unsafe { raw::ecs_field_id(iterator.0.as_ptr(), spec.index) };
    let is_pair = unsafe { raw::ecs_id_is_pair(field_id) };
    if is_pair != (spec.shape == Shape::Pair) {
        return Err(Error::Projection(ProjectionError::UnexpectedPair(
            spec.index,
        )));
    }

    let real_world = NonNull::new(iter.real_world)
        .or_else(|| NonNull::new(iter.world))
        .ok_or(Error::Projection(ProjectionError::NullField(spec.index)))?;
    validate_field_type(iterator, spec, real_world, field_id, is_pair)?;

    let pointer = field_pointer(iterator, iter, spec, row)?;
    if pointer.as_ptr().addr() % spec.alignment != 0 {
        return Err(Error::Projection(ProjectionError::AlignmentMismatch(
            spec.index,
        )));
    }

    Ok(ResolvedField {
        spec,
        pointer: Some(pointer),
        pair: is_pair.then(|| pair_parts(real_world, field_id)),
    })
}

fn validate_field_type(
    iterator: IterPtr,
    spec: FieldSpec,
    real_world: NonNull<raw::ecs_world_t>,
    field_id: u64,
    is_pair: bool,
) -> Result<(), Error> {
    let actual_type = if is_pair {
        unsafe { raw::ecs_get_typeid(real_world.as_ptr(), field_id) }
    } else {
        field_id
    };
    if actual_type != spec.component {
        return Err(Error::Projection(ProjectionError::ComponentMismatch(
            spec.index,
        )));
    }
    if unsafe { raw::ecs_field_size(iterator.0.as_ptr(), spec.index) } != spec.size {
        return Err(Error::Projection(ProjectionError::SizeMismatch(spec.index)));
    }
    Ok(())
}

fn field_pointer(
    iterator: IterPtr,
    iter: &raw::ecs_iter_t,
    spec: FieldSpec,
    row: i32,
) -> Result<NonNull<u8>, Error> {
    let index = u32::try_from(spec.index)
        .map_err(|_error| Error::Projection(ProjectionError::FieldOutOfRange(spec.index)))?;
    let bit =
        1_u32
            .checked_shl(index)
            .ok_or(Error::Projection(ProjectionError::FieldOutOfRange(
                spec.index,
            )))?;
    let pointer = if iter.row_fields & bit != 0 {
        unsafe { raw::ecs_field_at_w_size(iterator.0.as_ptr(), spec.size, spec.index, row) }
            .cast::<u8>()
    } else {
        let base = unsafe { raw::ecs_field_w_size(iterator.0.as_ptr(), spec.size, spec.index) }
            .cast::<u8>();
        if unsafe { raw::ecs_field_is_self(iterator.0.as_ptr(), spec.index) } {
            let row = usize::try_from(row).map_err(|_error| {
                Error::Projection(ProjectionError::FieldOutOfRange(spec.index))
            })?;
            unsafe { base.add(row.saturating_mul(spec.size)) }
        } else {
            base
        }
    };
    NonNull::new(pointer).ok_or(Error::Projection(ProjectionError::NullField(spec.index)))
}

fn validate_access(iterator: IterPtr, spec: FieldSpec) -> Result<(), Error> {
    match spec.access {
        Access::Read => {
            if unsafe { raw::ecs_field_is_writeonly(iterator.0.as_ptr(), spec.index) } {
                return Err(Error::Projection(ProjectionError::WriteOnly(spec.index)));
            }
        }
        Access::Write => {
            if unsafe { raw::ecs_field_is_readonly(iterator.0.as_ptr(), spec.index) } {
                return Err(Error::Projection(ProjectionError::ReadOnly(spec.index)));
            }
            if !unsafe { raw::ecs_field_is_self(iterator.0.as_ptr(), spec.index) } {
                return Err(Error::Projection(ProjectionError::SharedWrite(spec.index)));
            }
        }
    }
    Ok(())
}

fn pair_parts(world: NonNull<raw::ecs_world_t>, pair: u64) -> (u64, u64) {
    let component_bits = pair & raw::ECS_COMPONENT_MASK;
    let first = component_bits >> 32;
    let second = component_bits & u64::from(u32::MAX);
    (
        unsafe { raw::ecs_get_alive(world.as_ptr(), first) },
        unsafe { raw::ecs_get_alive(world.as_ptr(), second) },
    )
}

fn validate_aliases(fields: &[ResolvedField]) -> Result<(), Error> {
    for (left_index, left) in fields.iter().enumerate() {
        let Some(left_pointer) = left.pointer else {
            continue;
        };
        for right in &fields[left_index + 1..] {
            let Some(right_pointer) = right.pointer else {
                continue;
            };
            if left.spec.access == Access::Read && right.spec.access == Access::Read {
                continue;
            }

            let left_start = left_pointer.as_ptr().addr();
            let left_end = left_start.saturating_add(left.spec.size);
            let right_start = right_pointer.as_ptr().addr();
            let right_end = right_start.saturating_add(right.spec.size);
            if left_start < right_end && right_start < left_end {
                return Err(Error::Projection(ProjectionError::AliasedMutableFields(
                    left.spec.index,
                    right.spec.index,
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn query_each<P, F>(
    query: QueryPtr,
    world: WorldPtr,
    world_key: WorldKey,
    specs: &[FieldSpec],
    callback: &mut F,
) -> Result<(), Error>
where
    P: Projection,
    F: for<'a> FnMut(EntityId, P::Item<'a>) -> Result<(), Error>,
{
    let mut iterator = unsafe { raw::ecs_query_iter(world.0.as_ptr(), query.0.as_ptr()) };
    let iterator_ptr = IterPtr(NonNull::from(&mut iterator));
    let mut guard = IteratorGuard {
        iterator: iterator_ptr,
        exhausted: false,
    };

    loop {
        if !unsafe { raw::ecs_query_next(iterator_ptr.0.as_ptr()) } {
            guard.exhausted = true;
            break;
        }
        let count = iterator.count;
        for row in 0..count {
            let Ok(row_index) = usize::try_from(row) else {
                return Err(Error::Projection(ProjectionError::FieldOutOfRange(-1)));
            };
            let entity = unsafe { *iterator.entities.add(row_index) };
            let resolved = resolve_fields(iterator_ptr, row, specs)?;
            let item = unsafe { P::materialize(&resolved, world_key) };
            callback(
                EntityId {
                    raw: entity,
                    world: world_key,
                },
                item,
            )?;
        }
    }
    Ok(())
}

struct IteratorGuard {
    iterator: IterPtr,
    exhausted: bool,
}

impl Drop for IteratorGuard {
    fn drop(&mut self) {
        if !self.exhausted {
            unsafe { raw::ecs_iter_fini(self.iterator.0.as_ptr()) };
        }
    }
}

#[derive(Debug)]
pub(crate) struct FailureState {
    world: WorldKey,
    failed: AtomicBool,
    error: Mutex<Option<RunError>>,
}

impl FailureState {
    pub(crate) fn new(world: WorldKey) -> Arc<Self> {
        Arc::new(Self {
            world,
            failed: AtomicBool::new(false),
            error: Mutex::new(None),
        })
    }

    pub(crate) fn clear(&self) {
        self.failed.store(false, Ordering::Release);
        if let Ok(mut error) = self.error.lock() {
            *error = None;
        }
    }

    pub(crate) fn take(&self) -> Option<RunError> {
        self.error.lock().ok().and_then(|mut error| error.take())
    }

    fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    fn record(&self, error: RunError, kind: CallbackFailureKind) {
        if self
            .failed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let _telemetry_result = catch_unwind(AssertUnwindSafe(|| {
                telemetry::callback_failed(
                    self.world,
                    kind,
                    error.system().unwrap_or("unknown"),
                    error.message().unwrap_or("unknown callback failure"),
                );
            }));
            if let Ok(mut slot) = self.error.lock() {
                *slot = Some(error);
            }
        }
    }
}

/// Deferred structural access available to single-threaded systems.
pub struct Commands<'a> {
    stage: WorldPtr,
    world: WorldKey,
    marker: PhantomData<&'a mut ()>,
    not_send_sync: PhantomData<Rc<()>>,
}

impl Commands<'_> {
    pub fn spawn(&mut self) -> Result<EntityId, Error> {
        let raw =
            create_entity(self.stage, None).ok_or_else(|| Error::EntityCreation(String::new()))?;
        Ok(EntityId {
            raw,
            world: self.world,
        })
    }

    pub fn despawn(&mut self, entity: EntityId) -> Result<(), Error> {
        self.validate(entity)?;
        delete_entity(self.stage, entity.raw);
        Ok(())
    }

    pub fn insert<T: Component>(
        &mut self,
        entity: EntityId,
        component: ComponentId<T>,
        value: T,
    ) -> Result<(), Error> {
        self.validate(entity)?;
        if component.world != self.world {
            return Err(Error::WrongWorld);
        }
        set_component(self.stage, entity.raw, component.raw, &value);
        Ok(())
    }

    pub fn remove<T>(&mut self, entity: EntityId, component: ComponentId<T>) -> Result<(), Error> {
        self.validate(entity)?;
        if component.world != self.world {
            return Err(Error::WrongWorld);
        }
        remove_id(self.stage, entity.raw, component.raw);
        Ok(())
    }

    fn validate(&self, entity: EntityId) -> Result<(), Error> {
        if entity.world != self.world {
            return Err(Error::WrongWorld);
        }
        if !is_alive(self.stage, entity.raw) {
            return Err(Error::DeadEntity);
        }
        Ok(())
    }
}

/// Context supplied to a single-threaded system callback.
pub struct SystemContext<'a> {
    commands: Commands<'a>,
    delta: TickDelta,
}

impl<'a> SystemContext<'a> {
    #[must_use]
    pub fn delta(&self) -> TickDelta {
        self.delta
    }

    #[must_use]
    pub fn commands(&mut self) -> &mut Commands<'a> {
        &mut self.commands
    }
}

/// Restricted context supplied to a parallel system callback.
#[derive(Debug, Clone, Copy)]
pub struct ParallelContext {
    delta: TickDelta,
    worker_index: u32,
}

impl ParallelContext {
    #[must_use]
    pub fn delta(self) -> TickDelta {
        self.delta
    }

    #[must_use]
    pub fn worker_index(self) -> u32 {
        self.worker_index
    }
}

struct CallbackContext<P, F> {
    world: WorldKey,
    system_name: String,
    specs: Vec<FieldSpec>,
    failure: Arc<FailureState>,
    callback: F,
    marker: PhantomData<fn() -> P>,
}

pub(crate) struct SystemDefinition<'a> {
    pub(crate) world: WorldKey,
    pub(crate) entity: u64,
    pub(crate) expression: &'a CStr,
    pub(crate) phase: Option<u64>,
    pub(crate) specs: Vec<FieldSpec>,
    pub(crate) failure: Arc<FailureState>,
    pub(crate) name: String,
}

pub(crate) fn create_single_system<P, F>(
    world: WorldPtr,
    definition: SystemDefinition<'_>,
    callback: F,
) -> Option<u64>
where
    P: Projection + 'static,
    F: for<'a> Fn(SystemContext<'a>, EntityId, P::Item<'a>) -> SystemResult + 'static,
{
    let context = Box::new(CallbackContext::<P, F> {
        world: definition.world,
        system_name: definition.name,
        specs: definition.specs,
        failure: definition.failure,
        callback,
        marker: PhantomData,
    });
    let context_pointer = Box::into_raw(context).cast::<c_void>();
    let descriptor = raw::ecs_system_desc_t {
        entity: definition.entity,
        query: raw::ecs_query_desc_t {
            expr: definition.expression.as_ptr(),
            ..raw::ecs_query_desc_t::default()
        },
        phase: definition.phase.unwrap_or_default(),
        callback: Some(single_system_trampoline::<P, F>),
        callback_ctx: context_pointer,
        callback_ctx_free: Some(drop_callback_context::<P, F>),
        ..raw::ecs_system_desc_t::default()
    };
    let system = unsafe { raw::ecs_system_init(world.0.as_ptr(), &raw const descriptor) };
    if system == 0 {
        unsafe {
            drop(Box::from_raw(
                context_pointer.cast::<CallbackContext<P, F>>(),
            ))
        };
        None
    } else {
        Some(system)
    }
}

pub(crate) fn create_parallel_system<P, F>(
    world: WorldPtr,
    definition: SystemDefinition<'_>,
    callback: F,
) -> Option<u64>
where
    P: Projection + 'static,
    F: for<'a> Fn(ParallelContext, EntityId, P::Item<'a>) -> SystemResult + Send + Sync + 'static,
{
    let context = Box::new(CallbackContext::<P, F> {
        world: definition.world,
        system_name: definition.name,
        specs: definition.specs,
        failure: definition.failure,
        callback,
        marker: PhantomData,
    });
    let context_pointer = Box::into_raw(context).cast::<c_void>();
    let descriptor = raw::ecs_system_desc_t {
        entity: definition.entity,
        query: raw::ecs_query_desc_t {
            expr: definition.expression.as_ptr(),
            ..raw::ecs_query_desc_t::default()
        },
        phase: definition.phase.unwrap_or_default(),
        callback: Some(parallel_system_trampoline::<P, F>),
        callback_ctx: context_pointer,
        callback_ctx_free: Some(drop_callback_context::<P, F>),
        multi_threaded: true,
        ..raw::ecs_system_desc_t::default()
    };
    let system = unsafe { raw::ecs_system_init(world.0.as_ptr(), &raw const descriptor) };
    if system == 0 {
        unsafe {
            drop(Box::from_raw(
                context_pointer.cast::<CallbackContext<P, F>>(),
            ))
        };
        None
    } else {
        Some(system)
    }
}

unsafe extern "C" fn drop_callback_context<P, F>(context: *mut c_void) {
    if !context.is_null() {
        let context = unsafe { Box::from_raw(context.cast::<CallbackContext<P, F>>()) };
        let failure = Arc::clone(&context.failure);
        let system_name = context.system_name.clone();
        let result = catch_unwind(AssertUnwindSafe(|| drop(context)));
        if let Err(payload) = result {
            failure.record(
                RunError::new(
                    system_name,
                    format!(
                        "callback context destructor panicked: {}",
                        panic_message(payload.as_ref())
                    ),
                ),
                CallbackFailureKind::Panic,
            );
        }
    }
}

unsafe extern "C" fn single_system_trampoline<P, F>(iterator: *mut raw::ecs_iter_t)
where
    P: Projection + 'static,
    F: for<'a> Fn(SystemContext<'a>, EntityId, P::Item<'a>) -> SystemResult + 'static,
{
    let Some(iterator) = NonNull::new(iterator) else {
        return;
    };
    let context_pointer = unsafe { iterator.as_ref().callback_ctx };
    let Some(context) = context_pointer.cast::<CallbackContext<P, F>>().as_ref() else {
        return;
    };
    let Some(stage) = NonNull::new(unsafe { iterator.as_ref().world }) else {
        context.failure.record(
            RunError::new(
                context.system_name.clone(),
                "Flecs supplied a null stage".to_owned(),
            ),
            CallbackFailureKind::Internal,
        );
        return;
    };
    run_system_rows::<P, _>(iterator, context, |iter, entity, item| {
        let delta = TickDelta::from_flecs(unsafe { iter.as_ref().delta_time });
        let commands = Commands {
            stage: WorldPtr(stage),
            world: context.world,
            marker: PhantomData,
            not_send_sync: PhantomData,
        };
        (context.callback)(SystemContext { commands, delta }, entity, item)
    });
}

unsafe extern "C" fn parallel_system_trampoline<P, F>(iterator: *mut raw::ecs_iter_t)
where
    P: Projection + 'static,
    F: for<'a> Fn(ParallelContext, EntityId, P::Item<'a>) -> SystemResult + Send + Sync + 'static,
{
    let Some(iterator) = NonNull::new(iterator) else {
        return;
    };
    let context_pointer = unsafe { iterator.as_ref().callback_ctx };
    let Some(context) = context_pointer.cast::<CallbackContext<P, F>>().as_ref() else {
        return;
    };
    run_system_rows::<P, _>(iterator, context, |iter, entity, item| {
        let iter_ref = unsafe { iter.as_ref() };
        let worker = unsafe { raw::ecs_stage_get_id(iter_ref.world) };
        let worker_index = u32::try_from(worker).unwrap_or_default();
        (context.callback)(
            ParallelContext {
                delta: TickDelta::from_flecs(iter_ref.delta_time),
                worker_index,
            },
            entity,
            item,
        )
    });
}

fn run_system_rows<P, F>(
    iterator: NonNull<raw::ecs_iter_t>,
    context: &CallbackContext<P, impl Sized>,
    callback: F,
) where
    P: Projection,
    F: for<'a> Fn(NonNull<raw::ecs_iter_t>, EntityId, P::Item<'a>) -> SystemResult,
{
    if context.failure.has_failed() {
        return;
    }
    let iter_ptr = IterPtr(iterator);
    let iter = unsafe { iterator.as_ref() };
    for row in 0..iter.count {
        if context.failure.has_failed() {
            return;
        }
        if let Err(failure) = run_system_row(iterator, iter_ptr, iter, row, context, &callback) {
            context.failure.record(failure.error, failure.kind);
            return;
        }
    }
}

struct SystemRowFailure {
    error: RunError,
    kind: CallbackFailureKind,
}

fn run_system_row<P, F>(
    iterator: NonNull<raw::ecs_iter_t>,
    iter_ptr: IterPtr,
    iter: &raw::ecs_iter_t,
    row: i32,
    context: &CallbackContext<P, impl Sized>,
    callback: &F,
) -> Result<(), SystemRowFailure>
where
    P: Projection,
    F: for<'a> Fn(NonNull<raw::ecs_iter_t>, EntityId, P::Item<'a>) -> SystemResult,
{
    let row_index = usize::try_from(row).map_err(|error| {
        system_row_failure(context, error.to_string(), CallbackFailureKind::Internal)
    })?;
    let entity = EntityId {
        raw: unsafe { *iter.entities.add(row_index) },
        world: context.world,
    };
    let resolved = resolve_fields(iter_ptr, row, &context.specs).map_err(|error| {
        system_row_failure(context, error.to_string(), CallbackFailureKind::Projection)
    })?;
    let item = unsafe { P::materialize(&resolved, context.world) };
    match catch_unwind(AssertUnwindSafe(|| callback(iterator, entity, item))) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(system_row_failure(
            context,
            error.to_string(),
            CallbackFailureKind::Error,
        )),
        Err(payload) => Err(system_row_failure(
            context,
            panic_message(payload.as_ref()),
            CallbackFailureKind::Panic,
        )),
    }
}

fn system_row_failure<P>(
    context: &CallbackContext<P, impl Sized>,
    message: String,
    kind: CallbackFailureKind,
) -> SystemRowFailure {
    SystemRowFailure {
        error: RunError::new(context.system_name.clone(), message),
        kind,
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "panic with a non-string payload".to_owned()
    }
}

pub(crate) fn projection_specs<P: Projection>(
    projection: &P,
    resolve: &dyn Fn(TypeId) -> Option<u64>,
) -> Result<Vec<FieldSpec>, Error> {
    projection.specs(resolve)
}

pub(crate) fn phase_id(world: WorldKey, raw: u64) -> PhaseId {
    PhaseId { raw, world }
}

pub(crate) fn pipeline_id(world: WorldKey, raw: u64) -> PipelineId {
    PipelineId { raw, world }
}
