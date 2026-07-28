#![doc = include_str!("../README.md")]

mod component;
mod error;
mod ffi;
mod ids;
mod pipeline;
mod query;
mod system;
mod telemetry;
mod world;

pub use blackflower_ecs_derive::{Component, Tag};
pub use component::{Component, Tag};
pub use error::{Error, ProjectionError, RunError, SystemResult};
pub use ffi::{
    Commands, Optional, PairMut, PairRead, PairRef, PairWrite, ParallelContext, Projection, Read,
    SystemContext, Write,
};
pub use ids::{
    BuiltinPhase, ComponentId, EntityId, PhaseId, PipelineId, SystemId, TagId, TickDelta,
};
pub use pipeline::PipelineBuilder;
pub use query::{Query, QueryBuilder};
pub use system::{ProjectedSystemBuilder, SystemBuilder};
pub use world::{World, WorldBuilder};

/// The Flecs version compiled into this crate.
pub const FLECS_VERSION: (u32, u32, u32) = (
    ffi::raw::FLECS_VERSION_MAJOR,
    ffi::raw::FLECS_VERSION_MINOR,
    ffi::raw::FLECS_VERSION_PATCH,
);
