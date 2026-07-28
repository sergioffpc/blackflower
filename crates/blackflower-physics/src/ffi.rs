#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw Jolt calls and pointer materialization are isolated in this private module"
)]
#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block,
    reason = "all unsafe operations are confined to the reviewed Jolt FFI boundary"
)]

use std::ptr::NonNull;

use glam::Vec3A;

use crate::types::{BodySettings, ShapeKind};

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the blackflower Jolt C wrapper"
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
    include!(concat!(env!("OUT_DIR"), "/jolt_bindings.rs"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    InvalidArgument,
    InitializationFailed,
    BodyCapacityExhausted,
    BodyNotFound,
    ContractViolation,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WorldPtr(NonNull<raw::BFPhysicsWorld>);

pub(crate) struct WorldConfig {
    pub(crate) max_bodies: u32,
    pub(crate) body_mutexes: u32,
    pub(crate) max_body_pairs: u32,
    pub(crate) max_contact_constraints: u32,
    pub(crate) worker_threads: i32,
}

pub(crate) fn jolt_version() -> (u32, u32, u32) {
    let version = unsafe { raw::bf_physics_jolt_version() };
    (version.major, version.minor, version.patch)
}

pub(crate) fn create_world(config: WorldConfig) -> Result<WorldPtr, Status> {
    let config = raw::BFPhysicsWorldConfig {
        max_bodies: config.max_bodies,
        body_mutexes: config.body_mutexes,
        max_body_pairs: config.max_body_pairs,
        max_contact_constraints: config.max_contact_constraints,
        worker_threads: config.worker_threads,
    };
    let mut pointer = std::ptr::null_mut();
    let status = unsafe { raw::bf_physics_world_create(&raw const config, &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(WorldPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_world(world: WorldPtr) {
    unsafe { raw::bf_physics_world_destroy(world.0.as_ptr()) };
}

pub(crate) fn create_body(world: WorldPtr, settings: BodySettings) -> Result<u32, Status> {
    let raw_settings = raw::BFPhysicsBodySettings {
        position: raw_vec(settings.position),
        rotation: raw::BFPhysicsQuat {
            x: settings.rotation.x,
            y: settings.rotation.y,
            z: settings.rotation.z,
            w: settings.rotation.w,
        },
        motion_type: settings.motion_type.raw(),
        active: u8::from(settings.active),
    };
    let mut body = u32::MAX;
    let status = match settings.shape.kind() {
        ShapeKind::Sphere { radius } => unsafe {
            raw::bf_physics_world_create_sphere_body(
                world.0.as_ptr(),
                &raw const raw_settings,
                radius,
                &raw mut body,
            )
        },
        ShapeKind::Box { half_extent } => unsafe {
            raw::bf_physics_world_create_box_body(
                world.0.as_ptr(),
                &raw const raw_settings,
                raw_vec(half_extent),
                &raw mut body,
            )
        },
    };
    check(status)?;
    Ok(body)
}

pub(crate) fn destroy_body(world: WorldPtr, body: u32) -> Result<(), Status> {
    let status = unsafe { raw::bf_physics_world_destroy_body(world.0.as_ptr(), body) };
    check(status)
}

pub(crate) fn body_exists(world: WorldPtr, body: u32) -> Result<bool, Status> {
    let mut exists = 0;
    let status =
        unsafe { raw::bf_physics_world_body_exists(world.0.as_ptr(), body, &raw mut exists) };
    check(status)?;
    Ok(exists != 0)
}

pub(crate) fn body_is_active(world: WorldPtr, body: u32) -> Result<bool, Status> {
    let mut active = 0;
    let status =
        unsafe { raw::bf_physics_world_body_is_active(world.0.as_ptr(), body, &raw mut active) };
    check(status)?;
    Ok(active != 0)
}

pub(crate) fn body_position(world: WorldPtr, body: u32) -> Result<Vec3A, Status> {
    let mut position = raw::BFPhysicsVec3::default();
    let status =
        unsafe { raw::bf_physics_world_body_position(world.0.as_ptr(), body, &raw mut position) };
    check(status)?;
    Ok(safe_vec(position))
}

pub(crate) fn body_linear_velocity(world: WorldPtr, body: u32) -> Result<Vec3A, Status> {
    let mut velocity = raw::BFPhysicsVec3::default();
    let status = unsafe {
        raw::bf_physics_world_body_linear_velocity(world.0.as_ptr(), body, &raw mut velocity)
    };
    check(status)?;
    Ok(safe_vec(velocity))
}

pub(crate) fn set_body_linear_velocity(
    world: WorldPtr,
    body: u32,
    velocity: Vec3A,
) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_physics_world_set_body_linear_velocity(world.0.as_ptr(), body, raw_vec(velocity))
    };
    check(status)
}

pub(crate) fn optimize_broad_phase(world: WorldPtr) {
    unsafe { raw::bf_physics_world_optimize_broad_phase(world.0.as_ptr()) };
}

pub(crate) fn update(
    world: WorldPtr,
    delta_seconds: f32,
    collision_steps: i32,
) -> Result<u32, Status> {
    let mut update_errors = 0;
    let status = unsafe {
        raw::bf_physics_world_update(
            world.0.as_ptr(),
            delta_seconds,
            collision_steps,
            &raw mut update_errors,
        )
    };
    check(status)?;
    Ok(update_errors)
}

fn check(status: i32) -> Result<(), Status> {
    let Ok(status) = u32::try_from(status) else {
        return Err(Status::ContractViolation);
    };
    match status {
        raw::BF_PHYSICS_STATUS_OK => Ok(()),
        raw::BF_PHYSICS_STATUS_INVALID_ARGUMENT => Err(Status::InvalidArgument),
        raw::BF_PHYSICS_STATUS_INITIALIZATION_FAILED => Err(Status::InitializationFailed),
        raw::BF_PHYSICS_STATUS_BODY_CAPACITY_EXHAUSTED => Err(Status::BodyCapacityExhausted),
        raw::BF_PHYSICS_STATUS_BODY_NOT_FOUND => Err(Status::BodyNotFound),
        _ => Err(Status::ContractViolation),
    }
}

fn raw_vec(value: Vec3A) -> raw::BFPhysicsVec3 {
    raw::BFPhysicsVec3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

fn safe_vec(value: raw::BFPhysicsVec3) -> Vec3A {
    Vec3A::new(value.x, value.y, value.z)
}
