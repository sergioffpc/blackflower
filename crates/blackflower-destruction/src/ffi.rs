#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw Blast calls and native pointer materialization are isolated in this private module"
)]
#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block,
    reason = "all unsafe operations are confined to the reviewed Blast FFI boundary"
)]

use std::ffi::CStr;
use std::ptr::NonNull;

use glam::Vec3A;

use crate::{
    ActorId, BondDesc, ChunkDesc, Error, ForceMode, FractureCommand, FractureEvent, GraphNodeId,
    StressSettings, StressStats,
};

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the Blackflower destruction C API"
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
mod raw {
    include!(concat!(env!("OUT_DIR"), "/blast_bindings.rs"));
}

pub(crate) type AssetPointer = NonNull<raw::BFDestructionAsset>;
pub(crate) type FamilyPointer = NonNull<raw::BFDestructionFamily>;
pub(crate) type NativeChunk = raw::BFDestructionChunkDesc;
pub(crate) type NativeBond = raw::BFDestructionBondDesc;
pub(crate) type NativeFracture = raw::BFDestructionFractureData;

pub(crate) fn blast_version() -> &'static str {
    let pointer = unsafe { raw::bf_destruction_blast_version() };
    if pointer.is_null() {
        return "unknown";
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .unwrap_or("unknown")
}

pub(crate) fn stress_supported() -> bool {
    unsafe { raw::bf_destruction_stress_supported() != 0 }
}

pub(crate) fn create_asset(
    chunks: &[NativeChunk],
    bonds: &[NativeBond],
) -> Result<AssetPointer, Error> {
    let chunk_count = u32::try_from(chunks.len()).map_err(|_error| Error::InvalidChunk)?;
    let bond_count = u32::try_from(bonds.len()).map_err(|_error| Error::InvalidBond)?;
    let mut pointer = std::ptr::null_mut();
    let status = unsafe {
        raw::bf_destruction_asset_create(
            chunks.as_ptr(),
            chunk_count,
            if bonds.is_empty() {
                std::ptr::null()
            } else {
                bonds.as_ptr()
            },
            bond_count,
            &mut pointer,
        )
    };
    check(status)?;
    NonNull::new(pointer).ok_or(Error::NativeContract)
}

pub(crate) fn destroy_asset(pointer: AssetPointer) {
    unsafe { raw::bf_destruction_asset_destroy(pointer.as_ptr()) };
}

pub(crate) fn asset_chunk_count(pointer: AssetPointer) -> u32 {
    unsafe { raw::bf_destruction_asset_chunk_count(pointer.as_ptr()) }
}

pub(crate) fn asset_bond_count(pointer: AssetPointer) -> u32 {
    unsafe { raw::bf_destruction_asset_bond_count(pointer.as_ptr()) }
}

pub(crate) fn asset_support_chunk_count(pointer: AssetPointer) -> u32 {
    unsafe { raw::bf_destruction_asset_support_chunk_count(pointer.as_ptr()) }
}

pub(crate) fn asset_graph_node_count(pointer: AssetPointer) -> u32 {
    unsafe { raw::bf_destruction_asset_graph_node_count(pointer.as_ptr()) }
}

pub(crate) fn create_family(
    asset: AssetPointer,
    initial_bond_health: f32,
    initial_chunk_health: f32,
) -> Result<FamilyPointer, Error> {
    let mut pointer = std::ptr::null_mut();
    let status = unsafe {
        raw::bf_destruction_family_create(
            asset.as_ptr(),
            initial_bond_health,
            initial_chunk_health,
            &mut pointer,
        )
    };
    check(status)?;
    NonNull::new(pointer).ok_or(Error::NativeContract)
}

pub(crate) fn destroy_family(pointer: FamilyPointer) {
    unsafe { raw::bf_destruction_family_destroy(pointer.as_ptr()) };
}

pub(crate) fn actor_ids(pointer: FamilyPointer) -> Result<Vec<ActorId>, Error> {
    let capacity = unsafe { raw::bf_destruction_family_actor_count(pointer.as_ptr()) };
    let mut values = zeroed_u32(capacity)?;
    let mut count = 0u32;
    let status = unsafe {
        raw::bf_destruction_family_actor_ids(
            pointer.as_ptr(),
            mutable_or_null(&mut values),
            capacity,
            &mut count,
        )
    };
    check(status)?;
    truncate(&mut values, count)?;
    Ok(values.into_iter().map(ActorId::from_native).collect())
}

pub(crate) fn visible_chunks(
    pointer: FamilyPointer,
    actor: ActorId,
    capacity: u32,
) -> Result<Vec<u32>, Error> {
    let mut values = zeroed_u32(capacity)?;
    let mut count = 0u32;
    let status = unsafe {
        raw::bf_destruction_family_visible_chunks(
            pointer.as_ptr(),
            actor.get(),
            mutable_or_null(&mut values),
            capacity,
            &mut count,
        )
    };
    check(status)?;
    truncate(&mut values, count)?;
    Ok(values)
}

pub(crate) fn apply_fracture(
    pointer: FamilyPointer,
    actor: ActorId,
    commands: &[NativeFracture],
    capacity: u32,
) -> Result<Vec<FractureEvent>, Error> {
    let command_count = u32::try_from(commands.len()).map_err(|_error| Error::NativeContract)?;
    collect_events(capacity, |events, out_count| unsafe {
        raw::bf_destruction_family_apply_fracture(
            pointer.as_ptr(),
            actor.get(),
            if commands.is_empty() {
                std::ptr::null()
            } else {
                commands.as_ptr()
            },
            command_count,
            events,
            capacity,
            out_count,
        )
    })
}

pub(crate) fn split_actor(
    pointer: FamilyPointer,
    actor: ActorId,
    capacity: u32,
) -> Result<Vec<ActorId>, Error> {
    let mut values = zeroed_u32(capacity)?;
    let mut count = 0u32;
    let status = unsafe {
        raw::bf_destruction_family_split_actor(
            pointer.as_ptr(),
            actor.get(),
            mutable_or_null(&mut values),
            capacity,
            &mut count,
        )
    };
    check(status)?;
    truncate(&mut values, count)?;
    Ok(values.into_iter().map(ActorId::from_native).collect())
}

pub(crate) fn enable_stress(
    pointer: FamilyPointer,
    settings: StressSettings,
    density: f32,
) -> Result<(), Error> {
    let native = raw::BFDestructionStressSettings {
        max_solver_iterations_per_frame: settings.max_solver_iterations_per_frame,
        graph_reduction_level: settings.graph_reduction_level,
        compression_elastic_limit: settings.compression_elastic_limit,
        compression_fatal_limit: settings.compression_fatal_limit,
        tension_elastic_limit: settings.tension_elastic_limit,
        tension_fatal_limit: settings.tension_fatal_limit,
        shear_elastic_limit: settings.shear_elastic_limit,
        shear_fatal_limit: settings.shear_fatal_limit,
    };
    check(unsafe { raw::bf_destruction_family_enable_stress(pointer.as_ptr(), &native, density) })
}

pub(crate) fn stress_add_force(
    pointer: FamilyPointer,
    node: GraphNodeId,
    force: Vec3A,
    mode: ForceMode,
) -> Result<(), Error> {
    let mode = match mode {
        ForceMode::Force => raw::BF_DESTRUCTION_FORCE,
        ForceMode::Acceleration => raw::BF_DESTRUCTION_ACCELERATION,
    };
    check(unsafe {
        raw::bf_destruction_family_stress_add_force(
            pointer.as_ptr(),
            node.get(),
            native_vec3(force),
            mode,
        )
    })
}

pub(crate) fn stress_update(pointer: FamilyPointer) -> Result<(), Error> {
    check(unsafe { raw::bf_destruction_family_stress_update(pointer.as_ptr()) })
}

pub(crate) fn apply_stress(
    pointer: FamilyPointer,
    actor: ActorId,
    capacity: u32,
) -> Result<Vec<FractureEvent>, Error> {
    collect_events(capacity, |events, out_count| unsafe {
        raw::bf_destruction_family_apply_stress(
            pointer.as_ptr(),
            actor.get(),
            events,
            capacity,
            out_count,
        )
    })
}

pub(crate) fn stress_stats(pointer: FamilyPointer) -> Result<StressStats, Error> {
    let mut native = raw::BFDestructionStressStats::default();
    check(unsafe { raw::bf_destruction_family_stress_stats(pointer.as_ptr(), &mut native) })?;
    Ok(StressStats {
        frame_count: native.frame_count,
        bond_count: native.bond_count,
        overstressed_bond_count: native.overstressed_bond_count,
        linear_error: native.linear_error,
        angular_error: native.angular_error,
        converged: native.converged != 0,
    })
}

impl From<ChunkDesc> for NativeChunk {
    fn from(value: ChunkDesc) -> Self {
        Self {
            centroid: native_vec3(value.centroid),
            volume: value.volume,
            parent_chunk_index: value.parent.unwrap_or(raw::BF_DESTRUCTION_INVALID_INDEX),
            user_data: value.user_data,
            support: u8::from(value.support),
        }
    }
}

impl From<BondDesc> for NativeBond {
    fn from(value: BondDesc) -> Self {
        Self {
            normal: native_vec3(value.normal),
            area: value.area,
            centroid: native_vec3(value.centroid),
            user_data: value.user_data,
            chunk_index0: value.chunks[0].unwrap_or(raw::BF_DESTRUCTION_INVALID_INDEX),
            chunk_index1: value.chunks[1].unwrap_or(raw::BF_DESTRUCTION_INVALID_INDEX),
        }
    }
}

impl From<FractureCommand> for NativeFracture {
    fn from(value: FractureCommand) -> Self {
        match value {
            FractureCommand::Bond {
                first,
                second,
                damage,
            } => Self {
                kind: raw::BF_DESTRUCTION_FRACTURE_BOND,
                user_data: 0,
                index0: first.get(),
                index1: second.get(),
                health: damage,
            },
            FractureCommand::Chunk {
                chunk_index,
                damage,
            } => Self {
                kind: raw::BF_DESTRUCTION_FRACTURE_CHUNK,
                user_data: 0,
                index0: chunk_index,
                index1: raw::BF_DESTRUCTION_INVALID_INDEX,
                health: damage,
            },
        }
    }
}

fn native_vec3(value: Vec3A) -> raw::BFDestructionVec3 {
    raw::BFDestructionVec3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

fn collect_events(
    capacity: u32,
    operation: impl FnOnce(*mut NativeFracture, *mut u32) -> i32,
) -> Result<Vec<FractureEvent>, Error> {
    let length = usize::try_from(capacity).map_err(|_error| Error::NativeContract)?;
    let mut native = vec![NativeFracture::default(); length];
    let mut count = 0u32;
    let status = operation(mutable_or_null(&mut native), &mut count);
    check(status)?;
    truncate(&mut native, count)?;
    native.into_iter().map(fracture_event).collect()
}

fn fracture_event(value: NativeFracture) -> Result<FractureEvent, Error> {
    if value.kind == raw::BF_DESTRUCTION_FRACTURE_BOND {
        Ok(FractureEvent::Bond {
            first: GraphNodeId::new(value.index0),
            second: GraphNodeId::new(value.index1),
            user_data: value.user_data,
            remaining_health: value.health,
        })
    } else if value.kind == raw::BF_DESTRUCTION_FRACTURE_CHUNK {
        Ok(FractureEvent::Chunk {
            chunk_index: value.index0,
            user_data: value.user_data,
            remaining_health: value.health,
        })
    } else {
        Err(Error::NativeContract)
    }
}

fn zeroed_u32(capacity: u32) -> Result<Vec<u32>, Error> {
    usize::try_from(capacity)
        .map(|length| vec![0; length])
        .map_err(|_error| Error::NativeContract)
}

fn mutable_or_null<T>(values: &mut [T]) -> *mut T {
    if values.is_empty() {
        std::ptr::null_mut()
    } else {
        values.as_mut_ptr()
    }
}

fn truncate<T>(values: &mut Vec<T>, count: u32) -> Result<(), Error> {
    let length = usize::try_from(count).map_err(|_error| Error::NativeContract)?;
    if length > values.len() {
        return Err(Error::NativeContract);
    }
    values.truncate(length);
    Ok(())
}

fn check(status: i32) -> Result<(), Error> {
    if status == raw::BF_DESTRUCTION_STATUS_OK.cast_signed() {
        Ok(())
    } else if status == raw::BF_DESTRUCTION_STATUS_INVALID_ARGUMENT.cast_signed() {
        Err(Error::NativeContract)
    } else if status == raw::BF_DESTRUCTION_STATUS_ALLOCATION_FAILED.cast_signed() {
        Err(Error::AllocationFailed)
    } else if status == raw::BF_DESTRUCTION_STATUS_ASSET_CREATION_FAILED.cast_signed() {
        Err(Error::AssetCreation)
    } else if status == raw::BF_DESTRUCTION_STATUS_FAMILY_CREATION_FAILED.cast_signed() {
        Err(Error::FamilyCreation)
    } else if status == raw::BF_DESTRUCTION_STATUS_ACTOR_NOT_FOUND.cast_signed() {
        Err(Error::ActorNotFound)
    } else if status == raw::BF_DESTRUCTION_STATUS_STRESS_UNAVAILABLE.cast_signed() {
        Err(Error::StressUnavailable)
    } else if status == raw::BF_DESTRUCTION_STATUS_STRESS_FAILED.cast_signed() {
        Err(Error::StressNotReady)
    } else {
        Err(Error::NativeContract)
    }
}
