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

use glam::{Quat, Vec3A};

use crate::character::CharacterSettings;
use crate::shape::{CompoundShapeChild, Shape, ShapeKind};
use crate::types::BodySettings;

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
    CharacterNotFound,
    BodyOwnedByCharacter,
    ShapeCreationFailed,
    ContractViolation,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WorldPtr(NonNull<raw::BFPhysicsWorld>);

struct ShapePtr(NonNull<raw::BFPhysicsShape>);

impl ShapePtr {
    const fn as_ptr(&self) -> *mut raw::BFPhysicsShape {
        self.0.as_ptr()
    }
}

impl Drop for ShapePtr {
    fn drop(&mut self) {
        unsafe { raw::bf_physics_shape_destroy(self.0.as_ptr()) };
    }
}

pub(crate) struct WorldConfig {
    pub(crate) max_bodies: u32,
    pub(crate) body_mutexes: u32,
    pub(crate) max_body_pairs: u32,
    pub(crate) max_contact_constraints: u32,
    pub(crate) worker_threads: i32,
}

pub(crate) struct RawContactEvent {
    pub(crate) event: raw::BFPhysicsContactEvent,
    pub(crate) points: Vec<raw::BFPhysicsContactPoint>,
}

pub(crate) struct RawRayHit(pub(crate) raw::BFPhysicsRayHit);

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

pub(crate) fn create_body(world: WorldPtr, settings: &BodySettings) -> Result<u32, Status> {
    let raw_settings = raw::BFPhysicsBodySettings {
        position: raw_vec(settings.position),
        rotation: raw_quat(settings.rotation),
        motion_type: settings.motion_type.raw(),
        active: u8::from(settings.active),
    };
    let shape = create_shape(&settings.shape)?;
    let mut body = u32::MAX;
    let status = unsafe {
        raw::bf_physics_world_create_body(
            world.0.as_ptr(),
            &raw const raw_settings,
            shape.as_ptr(),
            &raw mut body,
        )
    };
    check(status)?;
    Ok(body)
}

fn create_shape(shape: &Shape) -> Result<ShapePtr, Status> {
    match shape.kind() {
        ShapeKind::Sphere { radius } => create_sphere(*radius),
        ShapeKind::Box { half_extent } => create_box(*half_extent),
        ShapeKind::Capsule {
            half_height,
            radius,
        } => create_capsule(*half_height, *radius),
        ShapeKind::ConvexHull { points } => create_convex_hull(points),
        ShapeKind::Compound { children } => create_compound(children),
        ShapeKind::TriangleMesh {
            vertices,
            triangles,
        } => create_triangle_mesh(vertices, triangles),
    }
}

fn create_sphere(radius: f32) -> Result<ShapePtr, Status> {
    let mut pointer = std::ptr::null_mut();
    let status = unsafe { raw::bf_physics_shape_create_sphere(radius, &raw mut pointer) };
    shape_result(status, pointer)
}

fn create_box(half_extent: Vec3A) -> Result<ShapePtr, Status> {
    let mut pointer = std::ptr::null_mut();
    let status =
        unsafe { raw::bf_physics_shape_create_box(raw_vec(half_extent), &raw mut pointer) };
    shape_result(status, pointer)
}

fn create_capsule(half_height: f32, radius: f32) -> Result<ShapePtr, Status> {
    let mut pointer = std::ptr::null_mut();
    let status =
        unsafe { raw::bf_physics_shape_create_capsule(half_height, radius, &raw mut pointer) };
    shape_result(status, pointer)
}

fn create_convex_hull(points: &[Vec3A]) -> Result<ShapePtr, Status> {
    let raw_points = points.iter().copied().map(raw_vec).collect::<Vec<_>>();
    let count = u32::try_from(raw_points.len()).map_err(|_error| Status::ContractViolation)?;
    let mut pointer = std::ptr::null_mut();
    let status = unsafe {
        raw::bf_physics_shape_create_convex_hull(raw_points.as_ptr(), count, &raw mut pointer)
    };
    shape_result(status, pointer)
}

fn create_compound(children: &[CompoundShapeChild]) -> Result<ShapePtr, Status> {
    let shapes = children
        .iter()
        .map(|child| create_shape(&child.shape))
        .collect::<Result<Vec<_>, _>>()?;
    let raw_children = children
        .iter()
        .zip(&shapes)
        .map(|(child, shape)| raw_compound_child(child, shape))
        .collect::<Vec<_>>();
    let count = u32::try_from(raw_children.len()).map_err(|_error| Status::ContractViolation)?;
    let mut pointer = std::ptr::null_mut();
    let status = unsafe {
        raw::bf_physics_shape_create_compound(raw_children.as_ptr(), count, &raw mut pointer)
    };
    shape_result(status, pointer)
}

fn raw_compound_child(child: &CompoundShapeChild, shape: &ShapePtr) -> raw::BFPhysicsCompoundChild {
    raw::BFPhysicsCompoundChild {
        shape: shape.as_ptr(),
        position: raw_vec(child.position),
        rotation: raw_quat(child.rotation),
    }
}

fn create_triangle_mesh(vertices: &[Vec3A], triangles: &[[u32; 3]]) -> Result<ShapePtr, Status> {
    let raw_vertices = vertices.iter().copied().map(raw_vec).collect::<Vec<_>>();
    let raw_triangles = triangles
        .iter()
        .copied()
        .map(raw_triangle)
        .collect::<Vec<_>>();
    create_triangle_mesh_from_raw(&raw_vertices, &raw_triangles)
}

fn create_triangle_mesh_from_raw(
    vertices: &[raw::BFPhysicsVec3],
    triangles: &[raw::BFPhysicsTriangle],
) -> Result<ShapePtr, Status> {
    let vertex_count = u32::try_from(vertices.len()).map_err(|_error| Status::ContractViolation)?;
    let triangle_count =
        u32::try_from(triangles.len()).map_err(|_error| Status::ContractViolation)?;
    let mut pointer = std::ptr::null_mut();
    let status = unsafe {
        raw::bf_physics_shape_create_triangle_mesh(
            vertices.as_ptr(),
            vertex_count,
            triangles.as_ptr(),
            triangle_count,
            &raw mut pointer,
        )
    };
    shape_result(status, pointer)
}

fn raw_triangle(indices: [u32; 3]) -> raw::BFPhysicsTriangle {
    raw::BFPhysicsTriangle {
        first: indices[0],
        second: indices[1],
        third: indices[2],
    }
}

fn shape_result(status: i32, pointer: *mut raw::BFPhysicsShape) -> Result<ShapePtr, Status> {
    check(status)?;
    NonNull::new(pointer)
        .map(ShapePtr)
        .ok_or(Status::ContractViolation)
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

pub(crate) fn body_rotation(world: WorldPtr, body: u32) -> Result<Quat, Status> {
    let mut rotation = raw::BFPhysicsQuat::default();
    let status =
        unsafe { raw::bf_physics_world_body_rotation(world.0.as_ptr(), body, &raw mut rotation) };
    check(status)?;
    Ok(safe_quat(rotation))
}

pub(crate) fn set_body_rotation(world: WorldPtr, body: u32, rotation: Quat) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_physics_world_set_body_rotation(world.0.as_ptr(), body, raw_quat(rotation))
    };
    check(status)
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

pub(crate) fn body_angular_velocity(world: WorldPtr, body: u32) -> Result<Vec3A, Status> {
    let mut velocity = raw::BFPhysicsVec3::default();
    let status = unsafe {
        raw::bf_physics_world_body_angular_velocity(world.0.as_ptr(), body, &raw mut velocity)
    };
    check(status)?;
    Ok(safe_vec(velocity))
}

pub(crate) fn set_body_angular_velocity(
    world: WorldPtr,
    body: u32,
    velocity: Vec3A,
) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_physics_world_set_body_angular_velocity(world.0.as_ptr(), body, raw_vec(velocity))
    };
    check(status)
}

pub(crate) fn add_body_force(world: WorldPtr, body: u32, force: Vec3A) -> Result<(), Status> {
    let status =
        unsafe { raw::bf_physics_world_add_body_force(world.0.as_ptr(), body, raw_vec(force)) };
    check(status)
}

pub(crate) fn add_body_force_at_point(
    world: WorldPtr,
    body: u32,
    force: Vec3A,
    point: Vec3A,
) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_physics_world_add_body_force_at_point(
            world.0.as_ptr(),
            body,
            raw_vec(force),
            raw_vec(point),
        )
    };
    check(status)
}

pub(crate) fn add_body_torque(world: WorldPtr, body: u32, torque: Vec3A) -> Result<(), Status> {
    let status =
        unsafe { raw::bf_physics_world_add_body_torque(world.0.as_ptr(), body, raw_vec(torque)) };
    check(status)
}

pub(crate) fn add_body_impulse(world: WorldPtr, body: u32, impulse: Vec3A) -> Result<(), Status> {
    let status =
        unsafe { raw::bf_physics_world_add_body_impulse(world.0.as_ptr(), body, raw_vec(impulse)) };
    check(status)
}

pub(crate) fn add_body_impulse_at_point(
    world: WorldPtr,
    body: u32,
    impulse: Vec3A,
    point: Vec3A,
) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_physics_world_add_body_impulse_at_point(
            world.0.as_ptr(),
            body,
            raw_vec(impulse),
            raw_vec(point),
        )
    };
    check(status)
}

pub(crate) fn add_body_angular_impulse(
    world: WorldPtr,
    body: u32,
    impulse: Vec3A,
) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_physics_world_add_body_angular_impulse(world.0.as_ptr(), body, raw_vec(impulse))
    };
    check(status)
}

pub(crate) fn create_character(
    world: WorldPtr,
    settings: CharacterSettings,
) -> Result<u32, Status> {
    let settings = raw::BFPhysicsCharacterSettings {
        position: raw_vec(settings.position),
        rotation: raw_quat(settings.rotation),
        capsule_half_height: settings.capsule_half_height,
        capsule_radius: settings.capsule_radius,
        mass: settings.mass,
        friction: settings.friction,
        gravity_factor: settings.gravity_factor,
        max_slope_angle_radians: settings.max_slope_angle_radians,
        active: u8::from(settings.active),
    };
    let mut character = u32::MAX;
    let status = unsafe {
        raw::bf_physics_world_create_character(
            world.0.as_ptr(),
            &raw const settings,
            &raw mut character,
        )
    };
    check(status)?;
    Ok(character)
}

pub(crate) fn destroy_character(world: WorldPtr, character: u32) -> Result<(), Status> {
    let status = unsafe { raw::bf_physics_world_destroy_character(world.0.as_ptr(), character) };
    check(status)
}

pub(crate) fn set_character_linear_velocity(
    world: WorldPtr,
    character: u32,
    velocity: Vec3A,
) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_physics_world_set_character_linear_velocity(
            world.0.as_ptr(),
            character,
            raw_vec(velocity),
        )
    };
    check(status)
}

pub(crate) fn refresh_character_ground_state(
    world: WorldPtr,
    character: u32,
    max_separation_distance: f32,
) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_physics_world_refresh_character_ground_state(
            world.0.as_ptr(),
            character,
            max_separation_distance,
        )
    };
    check(status)
}

pub(crate) fn character_state(
    world: WorldPtr,
    character: u32,
) -> Result<raw::BFPhysicsCharacterState, Status> {
    let mut state = raw::BFPhysicsCharacterState::default();
    let status = unsafe {
        raw::bf_physics_world_character_state(world.0.as_ptr(), character, &raw mut state)
    };
    check(status)?;
    Ok(state)
}

pub(crate) fn contact_events(world: WorldPtr) -> Result<Vec<RawContactEvent>, Status> {
    let mut count = 0;
    let status =
        unsafe { raw::bf_physics_world_contact_event_count(world.0.as_ptr(), &raw mut count) };
    check(status)?;
    let capacity = usize::try_from(count).map_err(|_error| Status::ContractViolation)?;
    let mut events = Vec::with_capacity(capacity);
    for event_index in 0..count {
        events.push(contact_event(world, event_index)?);
    }
    Ok(events)
}

fn contact_event(world: WorldPtr, event_index: u32) -> Result<RawContactEvent, Status> {
    let mut event = raw::BFPhysicsContactEvent::default();
    let status = unsafe {
        raw::bf_physics_world_contact_event(world.0.as_ptr(), event_index, &raw mut event)
    };
    check(status)?;
    let capacity =
        usize::try_from(event.point_count).map_err(|_error| Status::ContractViolation)?;
    let mut points = Vec::with_capacity(capacity);
    for point_index in 0..event.point_count {
        points.push(contact_point(world, event_index, point_index)?);
    }
    Ok(RawContactEvent { event, points })
}

pub(crate) fn cast_ray(
    world: WorldPtr,
    origin: Vec3A,
    displacement: Vec3A,
) -> Result<Option<RawRayHit>, Status> {
    let mut has_hit = 0;
    let mut hit = raw::BFPhysicsRayHit::default();
    let status = unsafe {
        raw::bf_physics_world_cast_ray(
            world.0.as_ptr(),
            raw_vec(origin),
            raw_vec(displacement),
            &raw mut has_hit,
            &raw mut hit,
        )
    };
    check(status)?;
    match has_hit {
        0 => Ok(None),
        1 => Ok(Some(RawRayHit(hit))),
        _ => Err(Status::ContractViolation),
    }
}

fn contact_point(
    world: WorldPtr,
    event_index: u32,
    point_index: u32,
) -> Result<raw::BFPhysicsContactPoint, Status> {
    let mut point = raw::BFPhysicsContactPoint::default();
    let status = unsafe {
        raw::bf_physics_world_contact_point(
            world.0.as_ptr(),
            event_index,
            point_index,
            &raw mut point,
        )
    };
    check(status)?;
    Ok(point)
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
        raw::BF_PHYSICS_STATUS_CHARACTER_NOT_FOUND => Err(Status::CharacterNotFound),
        raw::BF_PHYSICS_STATUS_BODY_OWNED_BY_CHARACTER => Err(Status::BodyOwnedByCharacter),
        raw::BF_PHYSICS_STATUS_SHAPE_CREATION_FAILED => Err(Status::ShapeCreationFailed),
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

pub(crate) fn safe_vec(value: raw::BFPhysicsVec3) -> Vec3A {
    Vec3A::new(value.x, value.y, value.z)
}

fn raw_quat(value: Quat) -> raw::BFPhysicsQuat {
    raw::BFPhysicsQuat {
        x: value.x,
        y: value.y,
        z: value.z,
        w: value.w,
    }
}

pub(crate) fn safe_quat(value: raw::BFPhysicsQuat) -> Quat {
    Quat::from_xyzw(value.x, value.y, value.z, value.w)
}
