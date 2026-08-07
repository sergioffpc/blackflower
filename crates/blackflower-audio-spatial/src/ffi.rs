#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw calls into the statically linked Steam Audio C API are isolated in this private module"
)]
use std::ptr::NonNull;

use glam::Vec3A;

use crate::types::{AudioSettings, BinauralParams, Interpolation, TailState};
use crate::{
    AcousticMaterial, AcousticProbe, BakedDataIdentifier, BakedDataType, BakedDataVariation,
    PathBakeSettings, ProbeVolumeTransform, RayTracerBackend, ReflectionsBakeSettings,
};

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the Steam Audio C API"
)]
#[allow(
    clippy::all,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::ptr_offset_with_cast,
    clippy::too_many_lines,
    clippy::upper_case_acronyms,
    clippy::useless_transmute,
    reason = "bindgen-generated code mirrors C layouts and is not maintained by hand"
)]
#[allow(
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks,
    reason = "bindgen output is generated from the pinned Steam Audio headers"
)]
pub(crate) mod raw {
    include!(concat!(env!("OUT_DIR"), "/steam_audio_bindings.rs"));
}

const STEAM_AUDIO_VERSION_PACKED: u32 = (4 << 16) | (8 << 8) | 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Failure,
    OutOfMemory,
    Initialization,
    ContractViolation,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextPtr(NonNull<raw::_IPLContext_t>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct HrtfPtr(NonNull<raw::_IPLHRTF_t>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct BinauralEffectPtr(NonNull<raw::_IPLBinauralEffect_t>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct EmbreeDevicePtr(NonNull<raw::_IPLEmbreeDevice_t>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScenePtr(NonNull<raw::_IPLScene_t>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct StaticMeshPtr(NonNull<raw::_IPLStaticMesh_t>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct InstancedMeshPtr(NonNull<raw::_IPLInstancedMesh_t>);

#[derive(Debug, Clone, Copy)]
struct SerializedObjectPtr(NonNull<raw::_IPLSerializedObject_t>);

#[derive(Debug, Clone, Copy)]
struct ProbeArrayPtr(NonNull<raw::_IPLProbeArray_t>);

struct ProbeArrayGuard(ProbeArrayPtr);

impl Drop for ProbeArrayGuard {
    fn drop(&mut self) {
        destroy_probe_array(self.0);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProbeBatchPtr(NonNull<raw::_IPLProbeBatch_t>);

// SAFETY: Steam Audio context objects are reference-counted and documented as
// usable across threads; the safe layer owns every retained reference.
unsafe impl Send for ContextPtr {}
// SAFETY: shared context operations are supported by Steam Audio and the safe
// layer does not expose mutable references through this opaque pointer.
unsafe impl Sync for ContextPtr {}
// SAFETY: HRTFs are immutable reference-counted datasets after construction.
unsafe impl Send for HrtfPtr {}
// SAFETY: HRTFs are immutable reference-counted datasets after construction.
unsafe impl Sync for HrtfPtr {}
// SAFETY: effect state may move between threads, while safe processing requires
// exclusive ownership and therefore prevents concurrent calls.
unsafe impl Send for BinauralEffectPtr {}
// SAFETY: the Embree device is reference-counted and documented for concurrent use.
unsafe impl Send for EmbreeDevicePtr {}
// SAFETY: the Embree device is reference-counted and documented for concurrent use.
unsafe impl Sync for EmbreeDevicePtr {}
// SAFETY: committed probe batches are immutable reference-counted datasets.
unsafe impl Send for ProbeBatchPtr {}
// SAFETY: committed probe batches are immutable reference-counted datasets.
unsafe impl Sync for ProbeBatchPtr {}

pub(crate) fn create_context() -> Result<ContextPtr, Status> {
    let mut settings = raw::IPLContextSettings {
        version: STEAM_AUDIO_VERSION_PACKED,
        logCallback: None,
        allocateCallback: None,
        freeCallback: None,
        simdLevel: maximum_simd_level(),
        flags: 0,
    };
    let mut pointer = std::ptr::null_mut();
    // SAFETY: `settings` is fully initialized and `pointer` is a valid, uniquely
    // writable out-parameter.
    let status = unsafe { raw::iplContextCreate(&raw mut settings, &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(ContextPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_context(context: ContextPtr) {
    let mut pointer = context.0.as_ptr();
    // SAFETY: this releases the safe owner's retained reference exactly once;
    // Steam Audio nulls the local pointer slot.
    unsafe { raw::iplContextRelease(&raw mut pointer) };
}

pub(crate) fn create_embree_device(context: ContextPtr) -> Result<EmbreeDevicePtr, Status> {
    let mut pointer = std::ptr::null_mut();
    // SAFETY: the context is live, the optional settings pointer is null by API
    // contract, and `pointer` is a valid, uniquely writable out-parameter.
    let status = unsafe {
        raw::iplEmbreeDeviceCreate(context.0.as_ptr(), std::ptr::null_mut(), &raw mut pointer)
    };
    check(status)?;
    NonNull::new(pointer)
        .map(EmbreeDevicePtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_embree_device(device: EmbreeDevicePtr) {
    let mut pointer = device.0.as_ptr();
    // SAFETY: this releases the safe owner's retained reference exactly once.
    unsafe { raw::iplEmbreeDeviceRelease(&raw mut pointer) };
}

pub(crate) fn create_scene(
    context: ContextPtr,
    embree: Option<EmbreeDevicePtr>,
) -> Result<ScenePtr, Status> {
    let mut settings = scene_settings(embree);
    let mut pointer = std::ptr::null_mut();
    // SAFETY: the context and optional Embree device are live, `settings` is
    // initialized, and `pointer` is uniquely writable.
    let status =
        unsafe { raw::iplSceneCreate(context.0.as_ptr(), &raw mut settings, &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(ScenePtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn load_scene(
    context: ContextPtr,
    embree: Option<EmbreeDevicePtr>,
    bytes: &[u8],
) -> Result<ScenePtr, Status> {
    let serialized = create_serialized_object(context, Some(bytes))?;
    let mut settings = scene_settings(embree);
    let mut pointer = std::ptr::null_mut();
    // SAFETY: all handles are live, settings remain readable, optional callbacks
    // are null as documented, and `pointer` is uniquely writable.
    let status = unsafe {
        raw::iplSceneLoad(
            context.0.as_ptr(),
            &raw mut settings,
            serialized.0.as_ptr(),
            None,
            std::ptr::null_mut(),
            &raw mut pointer,
        )
    };
    destroy_serialized_object(serialized);
    check(status)?;
    NonNull::new(pointer)
        .map(ScenePtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn save_scene(context: ContextPtr, scene: ScenePtr) -> Result<Vec<u8>, Status> {
    let serialized = create_serialized_object(context, None)?;
    // SAFETY: both handles are live and the serialized object is the uniquely
    // owned destination for this save operation.
    unsafe { raw::iplSceneSave(scene.0.as_ptr(), serialized.0.as_ptr()) };
    let bytes = serialized_object_bytes(serialized);
    destroy_serialized_object(serialized);
    bytes
}

pub(crate) fn destroy_scene(scene: ScenePtr) {
    let mut pointer = scene.0.as_ptr();
    // SAFETY: this releases the safe owner's retained scene reference exactly once.
    unsafe { raw::iplSceneRelease(&raw mut pointer) };
}

pub(crate) fn commit_scene(scene: ScenePtr) {
    // SAFETY: the scene is live and the safe owner serializes scene mutation/commit.
    unsafe { raw::iplSceneCommit(scene.0.as_ptr()) };
}

pub(crate) fn create_static_mesh(
    scene: ScenePtr,
    vertices: &[Vec3A],
    triangles: &[[i32; 3]],
    material_indices: &[i32],
    materials: &[AcousticMaterial],
) -> Result<StaticMeshPtr, Status> {
    let mut vertices = vertices.iter().copied().map(raw_vec).collect::<Vec<_>>();
    let mut triangles = triangles
        .iter()
        .copied()
        .map(|indices| raw::IPLTriangle { indices })
        .collect::<Vec<_>>();
    let mut material_indices = material_indices.to_vec();
    let mut materials = materials
        .iter()
        .copied()
        .map(raw_material)
        .collect::<Vec<_>>();
    let mut settings = raw::IPLStaticMeshSettings {
        numVertices: native_len(vertices.len()),
        numTriangles: native_len(triangles.len()),
        numMaterials: native_len(materials.len()),
        vertices: vertices.as_mut_ptr(),
        triangles: triangles.as_mut_ptr(),
        materialIndices: material_indices.as_mut_ptr(),
        materials: materials.as_mut_ptr(),
    };
    let mut pointer = std::ptr::null_mut();
    // SAFETY: the scene is live; all geometry/material slices referenced by
    // `settings` remain readable for their checked counts; `pointer` is writable.
    let status =
        unsafe { raw::iplStaticMeshCreate(scene.0.as_ptr(), &raw mut settings, &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(StaticMeshPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_static_mesh(mesh: StaticMeshPtr) {
    let mut pointer = mesh.0.as_ptr();
    // SAFETY: this releases the safe owner's retained mesh reference exactly once.
    unsafe { raw::iplStaticMeshRelease(&raw mut pointer) };
}

pub(crate) fn add_static_mesh(scene: ScenePtr, mesh: StaticMeshPtr) {
    // SAFETY: both handles are live and the safe scene owner serializes mutation.
    unsafe { raw::iplStaticMeshAdd(mesh.0.as_ptr(), scene.0.as_ptr()) };
}

pub(crate) fn remove_static_mesh(scene: ScenePtr, mesh: StaticMeshPtr) {
    // SAFETY: both handles are live and the safe scene owner serializes mutation.
    unsafe { raw::iplStaticMeshRemove(mesh.0.as_ptr(), scene.0.as_ptr()) };
}

pub(crate) fn create_instanced_mesh(
    scene: ScenePtr,
    sub_scene: ScenePtr,
    transform: [[f32; 4]; 4],
) -> Result<InstancedMeshPtr, Status> {
    let mut settings = raw::IPLInstancedMeshSettings {
        subScene: sub_scene.0.as_ptr(),
        transform: raw::IPLMatrix4x4 {
            elements: transform,
        },
    };
    let mut pointer = std::ptr::null_mut();
    // SAFETY: both scenes are live, `settings` is fully initialized, and
    // `pointer` is a valid, uniquely writable out-parameter.
    let status = unsafe {
        raw::iplInstancedMeshCreate(scene.0.as_ptr(), &raw mut settings, &raw mut pointer)
    };
    check(status)?;
    NonNull::new(pointer)
        .map(InstancedMeshPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_instanced_mesh(mesh: InstancedMeshPtr) {
    let mut pointer = mesh.0.as_ptr();
    // SAFETY: this releases the safe owner's retained instance reference exactly once.
    unsafe { raw::iplInstancedMeshRelease(&raw mut pointer) };
}

pub(crate) fn add_instanced_mesh(scene: ScenePtr, mesh: InstancedMeshPtr) {
    // SAFETY: both handles are live and the safe scene owner serializes mutation.
    unsafe { raw::iplInstancedMeshAdd(mesh.0.as_ptr(), scene.0.as_ptr()) };
}

pub(crate) fn remove_instanced_mesh(scene: ScenePtr, mesh: InstancedMeshPtr) {
    // SAFETY: both handles are live and the safe scene owner serializes mutation.
    unsafe { raw::iplInstancedMeshRemove(mesh.0.as_ptr(), scene.0.as_ptr()) };
}

pub(crate) fn update_instanced_mesh_transform(
    scene: ScenePtr,
    mesh: InstancedMeshPtr,
    transform: [[f32; 4]; 4],
) {
    // SAFETY: both handles are live, the instance belongs to the scene, and the
    // safe scene owner serializes transform mutation.
    unsafe {
        raw::iplInstancedMeshUpdateTransform(
            mesh.0.as_ptr(),
            scene.0.as_ptr(),
            raw::IPLMatrix4x4 {
                elements: transform,
            },
        )
    };
}

pub(crate) fn generate_uniform_floor_probes(
    context: ContextPtr,
    scene: ScenePtr,
    transform: ProbeVolumeTransform,
    spacing: f32,
    height: f32,
) -> Result<(ProbeBatchPtr, Vec<AcousticProbe>), Status> {
    let probe_array = ProbeArrayGuard(create_probe_array(context)?);
    let mut params = raw::IPLProbeGenerationParams {
        type_: raw::IPL_PROBEGENERATIONTYPE_UNIFORMFLOOR,
        spacing,
        height,
        transform: raw::IPLMatrix4x4 {
            elements: transform.rows(),
        },
    };
    // SAFETY: the probe array and scene are live, `params` is fully initialized,
    // and the safe owner serializes probe-array mutation.
    unsafe {
        raw::iplProbeArrayGenerateProbes(
            probe_array.0.0.as_ptr(),
            scene.0.as_ptr(),
            &raw mut params,
        );
    }
    // SAFETY: the probe array is live and generation completed before this query.
    let count = unsafe { raw::iplProbeArrayGetNumProbes(probe_array.0.0.as_ptr()) };
    let count = usize::try_from(count).map_err(|_error| Status::ContractViolation)?;
    let mut probes = Vec::with_capacity(count);
    for index in 0..count {
        let index = i32::try_from(index).map_err(|_error| Status::ContractViolation)?;
        // SAFETY: `index` is within the count returned by this live probe array.
        let sphere = unsafe { raw::iplProbeArrayGetProbe(probe_array.0.0.as_ptr(), index) };
        probes.push(
            AcousticProbe::new(
                Vec3A::new(sphere.center.x, sphere.center.y, sphere.center.z),
                sphere.radius,
            )
            .map_err(|_error| Status::ContractViolation)?,
        );
    }
    let batch = create_probe_batch(context)?;
    // SAFETY: both handles are live and the batch is exclusively owned while
    // adding the generated array.
    unsafe {
        raw::iplProbeBatchAddProbeArray(batch.0.as_ptr(), probe_array.0.0.as_ptr());
    }
    // SAFETY: the batch is live, exclusively owned, and all probe additions are complete.
    unsafe { raw::iplProbeBatchCommit(batch.0.as_ptr()) };
    Ok((batch, probes))
}

pub(crate) fn bake_reflections(
    context: ContextPtr,
    scene: ScenePtr,
    backend: RayTracerBackend,
    batch: ProbeBatchPtr,
    identifier: BakedDataIdentifier,
    settings: ReflectionsBakeSettings,
) {
    let mut params = raw::IPLReflectionsBakeParams {
        scene: scene.0.as_ptr(),
        probeBatch: batch.0.as_ptr(),
        sceneType: match backend {
            RayTracerBackend::BuiltIn => raw::IPL_SCENETYPE_DEFAULT,
            RayTracerBackend::Embree => raw::IPL_SCENETYPE_EMBREE,
        },
        identifier: raw_identifier(identifier),
        bakeFlags: raw::IPL_REFLECTIONSBAKEFLAGS_BAKECONVOLUTION
            | raw::IPL_REFLECTIONSBAKEFLAGS_BAKEPARAMETRIC,
        numRays: native_u32(settings.num_rays),
        numDiffuseSamples: native_u32(settings.num_diffuse_samples),
        numBounces: native_u32(settings.num_bounces),
        simulatedDuration: settings.simulated_duration,
        savedDuration: settings.saved_duration,
        order: native_u32(settings.order),
        numThreads: native_u32(settings.num_threads),
        rayBatchSize: native_u32(settings.ray_batch_size),
        irradianceMinDistance: settings.irradiance_min_distance,
        bakeBatchSize: native_u32(settings.bake_batch_size),
        openCLDevice: std::ptr::null_mut(),
        radeonRaysDevice: std::ptr::null_mut(),
    };
    // SAFETY: all handles are live, `params` remains readable, callbacks are
    // absent as documented for reflections baking, and the call is synchronous.
    unsafe {
        raw::iplReflectionsBakerBake(
            context.0.as_ptr(),
            &raw mut params,
            None,
            std::ptr::null_mut(),
        );
    }
}

pub(crate) fn bake_pathing(
    context: ContextPtr,
    scene: ScenePtr,
    batch: ProbeBatchPtr,
    identifier: BakedDataIdentifier,
    settings: PathBakeSettings,
) {
    let mut params = raw::IPLPathBakeParams {
        scene: scene.0.as_ptr(),
        probeBatch: batch.0.as_ptr(),
        identifier: raw_identifier(identifier),
        numSamples: native_u32(settings.num_samples),
        radius: settings.radius,
        threshold: settings.threshold,
        visRange: settings.visibility_range,
        pathRange: settings.path_range,
        numThreads: native_u32(settings.num_threads),
    };
    // SAFETY: all handles are live, `params` remains readable, the callback is
    // ABI-compatible and does not dereference user data, and the call is synchronous.
    unsafe {
        // Steam Audio 4.8.1 documents this callback as optional, but its
        // path-baker worker invokes it unconditionally.
        raw::iplPathBakerBake(
            context.0.as_ptr(),
            &raw mut params,
            Some(ignore_bake_progress),
            std::ptr::null_mut(),
        );
    }
}

unsafe extern "C" fn ignore_bake_progress(_progress: f32, _user_data: *mut std::ffi::c_void) {}

pub(crate) fn probe_batch_data_size(
    batch: ProbeBatchPtr,
    identifier: BakedDataIdentifier,
) -> usize {
    let mut identifier = raw_identifier(identifier);
    // SAFETY: the batch is live and `identifier` is a fully initialized input record.
    unsafe { raw::iplProbeBatchGetDataSize(batch.0.as_ptr(), &raw mut identifier) }
}

pub(crate) fn save_probe_batch(
    context: ContextPtr,
    batch: ProbeBatchPtr,
) -> Result<Vec<u8>, Status> {
    let serialized = create_serialized_object(context, None)?;
    // SAFETY: both handles are live and the serialized object is the uniquely
    // owned destination for this save operation.
    unsafe { raw::iplProbeBatchSave(batch.0.as_ptr(), serialized.0.as_ptr()) };
    let bytes = serialized_object_bytes(serialized);
    destroy_serialized_object(serialized);
    bytes
}

pub(crate) fn load_probe_batch(context: ContextPtr, bytes: &[u8]) -> Result<ProbeBatchPtr, Status> {
    let serialized = create_serialized_object(context, Some(bytes))?;
    let mut pointer = std::ptr::null_mut();
    // SAFETY: both input handles are live and `pointer` is a valid, uniquely
    // writable out-parameter.
    let status = unsafe {
        raw::iplProbeBatchLoad(context.0.as_ptr(), serialized.0.as_ptr(), &raw mut pointer)
    };
    destroy_serialized_object(serialized);
    check(status)?;
    NonNull::new(pointer)
        .map(ProbeBatchPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn probe_batch_count(batch: ProbeBatchPtr) -> Result<usize, ()> {
    // SAFETY: the probe batch is live and committed before shared queries.
    usize::try_from(unsafe { raw::iplProbeBatchGetNumProbes(batch.0.as_ptr()) }).map_err(drop)
}

pub(crate) fn destroy_probe_batch(batch: ProbeBatchPtr) {
    let mut pointer = batch.0.as_ptr();
    // SAFETY: this releases the safe owner's retained batch reference exactly once.
    unsafe { raw::iplProbeBatchRelease(&raw mut pointer) };
}

pub(crate) fn create_default_hrtf(
    context: ContextPtr,
    audio: AudioSettings,
) -> Result<HrtfPtr, Status> {
    let mut audio = raw_audio_settings(audio);
    let mut settings = raw::IPLHRTFSettings {
        type_: raw::IPL_HRTFTYPE_DEFAULT,
        sofaFileName: std::ptr::null(),
        sofaData: std::ptr::null(),
        sofaDataSize: 0,
        volume: 1.0,
        normType: raw::IPL_HRTFNORMTYPE_NONE,
    };
    let mut pointer = std::ptr::null_mut();
    // SAFETY: the context is live, both settings records are initialized, and
    // `pointer` is a valid, uniquely writable out-parameter.
    let status = unsafe {
        raw::iplHRTFCreate(
            context.0.as_ptr(),
            &raw mut audio,
            &raw mut settings,
            &raw mut pointer,
        )
    };
    check(status)?;
    NonNull::new(pointer)
        .map(HrtfPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_hrtf(hrtf: HrtfPtr) {
    let mut pointer = hrtf.0.as_ptr();
    // SAFETY: this releases the safe owner's retained HRTF reference exactly once.
    unsafe { raw::iplHRTFRelease(&raw mut pointer) };
}

pub(crate) fn create_binaural_effect(
    context: ContextPtr,
    hrtf: HrtfPtr,
    audio: AudioSettings,
) -> Result<BinauralEffectPtr, Status> {
    let mut audio = raw_audio_settings(audio);
    let mut settings = raw::IPLBinauralEffectSettings {
        hrtf: hrtf.0.as_ptr(),
    };
    let mut pointer = std::ptr::null_mut();
    // SAFETY: context and HRTF handles are live, settings are initialized, and
    // `pointer` is a valid, uniquely writable out-parameter.
    let status = unsafe {
        raw::iplBinauralEffectCreate(
            context.0.as_ptr(),
            &raw mut audio,
            &raw mut settings,
            &raw mut pointer,
        )
    };
    check(status)?;
    NonNull::new(pointer)
        .map(BinauralEffectPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_binaural_effect(effect: BinauralEffectPtr) {
    let mut pointer = effect.0.as_ptr();
    // SAFETY: this releases the safe owner's retained effect reference exactly once.
    unsafe { raw::iplBinauralEffectRelease(&raw mut pointer) };
}

pub(crate) fn reset_binaural_effect(effect: BinauralEffectPtr) {
    // SAFETY: the effect is live and the safe owner provides exclusive access.
    unsafe { raw::iplBinauralEffectReset(effect.0.as_ptr()) };
}

#[allow(
    clippy::too_many_arguments,
    reason = "the private FFI adapter names each Steam Audio input and output buffer explicitly"
)]
pub(crate) fn apply_binaural_effect(
    effect: BinauralEffectPtr,
    hrtf: HrtfPtr,
    audio: AudioSettings,
    params: BinauralParams,
    input: &[f32],
    output_left: &mut [f32],
    output_right: &mut [f32],
) -> Result<TailState, Status> {
    let mut input_channels = [input.as_ptr().cast_mut()];
    let mut output_channels = [output_left.as_mut_ptr(), output_right.as_mut_ptr()];
    let mut input_buffer = raw::IPLAudioBuffer {
        numChannels: 1,
        numSamples: audio.raw_frame_size(),
        data: input_channels.as_mut_ptr(),
    };
    let mut output_buffer = raw::IPLAudioBuffer {
        numChannels: 2,
        numSamples: audio.raw_frame_size(),
        data: output_channels.as_mut_ptr(),
    };
    let mut effect_params = raw::IPLBinauralEffectParams {
        direction: raw_vec(params.direction()),
        interpolation: raw_interpolation(params.interpolation()),
        spatialBlend: params.spatial_blend(),
        hrtf: hrtf.0.as_ptr(),
        peakDelays: std::ptr::null_mut(),
    };
    // SAFETY: effect and HRTF handles are live, channel pointer tables reference
    // buffers sized for the configured frame, and effect access is exclusive.
    let state = unsafe {
        raw::iplBinauralEffectApply(
            effect.0.as_ptr(),
            &raw mut effect_params,
            &raw mut input_buffer,
            &raw mut output_buffer,
        )
    };
    tail_state(state)
}

pub(crate) fn get_binaural_tail(
    effect: BinauralEffectPtr,
    audio: AudioSettings,
    output_left: &mut [f32],
    output_right: &mut [f32],
) -> Result<TailState, Status> {
    let mut output_channels = [output_left.as_mut_ptr(), output_right.as_mut_ptr()];
    let mut output_buffer = raw::IPLAudioBuffer {
        numChannels: 2,
        numSamples: audio.raw_frame_size(),
        data: output_channels.as_mut_ptr(),
    };
    // SAFETY: the effect is live, output channel pointers reference writable
    // frame-sized buffers, and effect access is exclusive.
    let state = unsafe { raw::iplBinauralEffectGetTail(effect.0.as_ptr(), &raw mut output_buffer) };
    tail_state(state)
}

pub(crate) fn binaural_tail_size(effect: BinauralEffectPtr) -> i32 {
    // SAFETY: the effect is live and this query does not mutate its processing state.
    unsafe { raw::iplBinauralEffectGetTailSize(effect.0.as_ptr()) }
}

fn raw_audio_settings(settings: AudioSettings) -> raw::IPLAudioSettings {
    raw::IPLAudioSettings {
        samplingRate: settings.raw_sampling_rate(),
        frameSize: settings.raw_frame_size(),
    }
}

fn raw_vec(value: Vec3A) -> raw::IPLVector3 {
    raw::IPLVector3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

fn raw_material(material: AcousticMaterial) -> raw::IPLMaterial {
    raw::IPLMaterial {
        absorption: material.absorption(),
        scattering: material.scattering(),
        transmission: material.transmission(),
    }
}

fn scene_settings(embree: Option<EmbreeDevicePtr>) -> raw::IPLSceneSettings {
    raw::IPLSceneSettings {
        type_: if embree.is_some() {
            raw::IPL_SCENETYPE_EMBREE
        } else {
            raw::IPL_SCENETYPE_DEFAULT
        },
        closestHitCallback: None,
        anyHitCallback: None,
        batchedClosestHitCallback: None,
        batchedAnyHitCallback: None,
        userData: std::ptr::null_mut(),
        embreeDevice: embree.map_or(std::ptr::null_mut(), |device| device.0.as_ptr()),
        radeonRaysDevice: std::ptr::null_mut(),
    }
}

fn create_serialized_object(
    context: ContextPtr,
    bytes: Option<&[u8]>,
) -> Result<SerializedObjectPtr, Status> {
    let (data, size) = bytes.map_or((std::ptr::null_mut(), 0), |bytes| {
        (bytes.as_ptr().cast_mut(), bytes.len())
    });
    let mut settings = raw::IPLSerializedObjectSettings { data, size };
    let mut pointer = std::ptr::null_mut();
    // SAFETY: the context is live, optional input bytes remain readable for
    // `size`, and `pointer` is a valid, uniquely writable out-parameter.
    let status = unsafe {
        raw::iplSerializedObjectCreate(context.0.as_ptr(), &raw mut settings, &raw mut pointer)
    };
    check(status)?;
    NonNull::new(pointer)
        .map(SerializedObjectPtr)
        .ok_or(Status::ContractViolation)
}

fn serialized_object_bytes(serialized: SerializedObjectPtr) -> Result<Vec<u8>, Status> {
    // SAFETY: the serialized object is live and owns its byte storage.
    let size = unsafe { raw::iplSerializedObjectGetSize(serialized.0.as_ptr()) };
    // SAFETY: the serialized object is live and owns its byte storage.
    let data = unsafe { raw::iplSerializedObjectGetData(serialized.0.as_ptr()) };
    if size == 0 || data.is_null() {
        return Err(Status::ContractViolation);
    }
    // SAFETY: the non-null pointer above addresses `size` bytes owned by the
    // still-live serialized object; the bytes are copied before release.
    Ok(unsafe { std::slice::from_raw_parts(data, size) }.to_vec())
}

fn destroy_serialized_object(serialized: SerializedObjectPtr) {
    let mut pointer = serialized.0.as_ptr();
    // SAFETY: this releases the uniquely owned serialized-object reference once.
    unsafe { raw::iplSerializedObjectRelease(&raw mut pointer) };
}

fn create_probe_array(context: ContextPtr) -> Result<ProbeArrayPtr, Status> {
    let mut pointer = std::ptr::null_mut();
    // SAFETY: the context is live and `pointer` is a valid, uniquely writable
    // out-parameter.
    let status = unsafe { raw::iplProbeArrayCreate(context.0.as_ptr(), &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(ProbeArrayPtr)
        .ok_or(Status::ContractViolation)
}

fn destroy_probe_array(array: ProbeArrayPtr) {
    let mut pointer = array.0.as_ptr();
    // SAFETY: this releases the uniquely owned probe-array reference once.
    unsafe { raw::iplProbeArrayRelease(&raw mut pointer) };
}

fn create_probe_batch(context: ContextPtr) -> Result<ProbeBatchPtr, Status> {
    let mut pointer = std::ptr::null_mut();
    // SAFETY: the context is live and `pointer` is a valid, uniquely writable
    // out-parameter.
    let status = unsafe { raw::iplProbeBatchCreate(context.0.as_ptr(), &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(ProbeBatchPtr)
        .ok_or(Status::ContractViolation)
}

fn raw_identifier(identifier: BakedDataIdentifier) -> raw::IPLBakedDataIdentifier {
    let endpoint = identifier.endpoint();
    raw::IPLBakedDataIdentifier {
        type_: match identifier.data_type() {
            BakedDataType::Reflections => raw::IPL_BAKEDDATATYPE_REFLECTIONS,
            BakedDataType::Pathing => raw::IPL_BAKEDDATATYPE_PATHING,
        },
        variation: match identifier.variation() {
            BakedDataVariation::Reverb => raw::IPL_BAKEDDATAVARIATION_REVERB,
            BakedDataVariation::StaticSource => raw::IPL_BAKEDDATAVARIATION_STATICSOURCE,
            BakedDataVariation::StaticListener => raw::IPL_BAKEDDATAVARIATION_STATICLISTENER,
            BakedDataVariation::Dynamic => raw::IPL_BAKEDDATAVARIATION_DYNAMIC,
        },
        endpointInfluence: raw::IPLSphere {
            center: raw_vec(endpoint.position()),
            radius: endpoint.radius(),
        },
    }
}

fn native_len(len: usize) -> i32 {
    i32::try_from(len)
        .unwrap_or_else(|_error| unreachable!("scene geometry validates native lengths"))
}

fn native_u32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or_else(|_error| {
        unreachable!("validated acoustic bake settings must fit the native range")
    })
}

const fn raw_interpolation(interpolation: Interpolation) -> raw::IPLHRTFInterpolation {
    match interpolation {
        Interpolation::Nearest => raw::IPL_HRTFINTERPOLATION_NEAREST,
        Interpolation::Bilinear => raw::IPL_HRTFINTERPOLATION_BILINEAR,
    }
}

fn tail_state(state: raw::IPLAudioEffectState) -> Result<TailState, Status> {
    match state {
        raw::IPL_AUDIOEFFECTSTATE_TAILREMAINING => Ok(TailState::Remaining),
        raw::IPL_AUDIOEFFECTSTATE_TAILCOMPLETE => Ok(TailState::Complete),
        _ => Err(Status::ContractViolation),
    }
}

fn check(status: raw::IPLerror) -> Result<(), Status> {
    match status {
        raw::IPL_STATUS_SUCCESS => Ok(()),
        raw::IPL_STATUS_FAILURE => Err(Status::Failure),
        raw::IPL_STATUS_OUTOFMEMORY => Err(Status::OutOfMemory),
        raw::IPL_STATUS_INITIALIZATION => Err(Status::Initialization),
        _ => Err(Status::ContractViolation),
    }
}

#[cfg(target_arch = "x86_64")]
const fn maximum_simd_level() -> raw::IPLSIMDLevel {
    raw::IPL_SIMDLEVEL_AVX2
}

#[cfg(target_arch = "aarch64")]
const fn maximum_simd_level() -> raw::IPLSIMDLevel {
    raw::IPL_SIMDLEVEL_NEON
}
