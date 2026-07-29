#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw calls into the statically linked Steam Audio C API are isolated in this private module"
)]
#![allow(
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks,
    reason = "all unsafe operations are confined to the reviewed Steam Audio FFI boundary"
)]

use std::ptr::NonNull;

use glam::Vec3A;

use crate::types::{AudioSettings, BinauralParams, Interpolation, TailState};

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

// Steam Audio documents its API objects as reference-counted and usable from
// multiple threads. Safe methods still require `&mut` for stateful effects.
unsafe impl Send for ContextPtr {}
unsafe impl Sync for ContextPtr {}
unsafe impl Send for HrtfPtr {}
unsafe impl Sync for HrtfPtr {}
unsafe impl Send for BinauralEffectPtr {}

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
    let status = unsafe { raw::iplContextCreate(&raw mut settings, &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(ContextPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_context(context: ContextPtr) {
    let mut pointer = context.0.as_ptr();
    unsafe { raw::iplContextRelease(&raw mut pointer) };
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
    unsafe { raw::iplBinauralEffectRelease(&raw mut pointer) };
}

pub(crate) fn reset_binaural_effect(effect: BinauralEffectPtr) {
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
    let state = unsafe { raw::iplBinauralEffectGetTail(effect.0.as_ptr(), &raw mut output_buffer) };
    tail_state(state)
}

pub(crate) fn binaural_tail_size(effect: BinauralEffectPtr) -> i32 {
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
