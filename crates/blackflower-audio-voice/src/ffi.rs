#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw calls into the statically linked Opus C API are isolated in this private module"
)]
use std::ffi::CStr;
use std::ptr::NonNull;

use crate::{Application, Channels, SampleRate};

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the Opus C API"
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
    reason = "bindgen output is generated from the pinned Opus headers"
)]
mod raw {
    include!(concat!(env!("OUT_DIR"), "/opus_bindings.rs"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    BadArgument,
    BufferTooSmall,
    Internal,
    InvalidPacket,
    Unimplemented,
    InvalidState,
    AllocationFailed,
    Unknown(i32),
    ContractViolation,
}

#[derive(Debug)]
pub(crate) struct EncoderPtr(NonNull<raw::OpusEncoder>);

#[derive(Debug)]
pub(crate) struct DecoderPtr(NonNull<raw::OpusDecoder>);

// SAFETY: Opus permits independent codec states to move between threads and
// safe access requires `&mut EncoderPtr`, preventing concurrent use of one state.
unsafe impl Send for EncoderPtr {}
// SAFETY: Opus permits independent codec states to move between threads and
// safe access requires `&mut DecoderPtr`, preventing concurrent use of one state.
unsafe impl Send for DecoderPtr {}

pub(crate) fn create_encoder(
    sample_rate: SampleRate,
    channels: Channels,
    application: Application,
) -> Result<EncoderPtr, Status> {
    let mut error = native_request(raw::OPUS_OK);
    // SAFETY: all scalar parameters are validated typed values and `error` is a
    // uniquely writable out-parameter for the duration of the call.
    let pointer = unsafe {
        raw::opus_encoder_create(
            sample_rate.native(),
            channels.native(),
            raw_application(application),
            &raw mut error,
        )
    };
    check(error)?;
    NonNull::new(pointer)
        .map(EncoderPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_encoder(encoder: &mut EncoderPtr) {
    // SAFETY: the safe owner invokes this matching destructor once and never
    // exposes the pointer again afterwards.
    unsafe { raw::opus_encoder_destroy(encoder.0.as_ptr()) };
}

pub(crate) fn encode_float(
    encoder: &mut EncoderPtr,
    input: &[f32],
    samples_per_channel: i32,
    output: &mut [u8],
) -> Result<usize, Status> {
    let output_length = native_length(output.len())?;
    // SAFETY: the encoder is exclusively borrowed, Opus derives the readable
    // PCM length from `samples_per_channel` and its channel count, and `output`
    // is writable for `output_length` bytes.
    let result = unsafe {
        raw::opus_encode_float(
            encoder.0.as_ptr(),
            input.as_ptr(),
            samples_per_channel,
            output.as_mut_ptr(),
            output_length,
        )
    };
    output_size(result)
}

pub(crate) fn set_bitrate(encoder: &mut EncoderPtr, bitrate: i32) -> Result<(), Status> {
    encoder_ctl_value(encoder, raw::OPUS_SET_BITRATE_REQUEST, bitrate)
}

pub(crate) fn set_complexity(encoder: &mut EncoderPtr, complexity: i32) -> Result<(), Status> {
    encoder_ctl_value(encoder, raw::OPUS_SET_COMPLEXITY_REQUEST, complexity)
}

pub(crate) fn set_vbr(encoder: &mut EncoderPtr, enabled: bool) -> Result<(), Status> {
    encoder_ctl_value(encoder, raw::OPUS_SET_VBR_REQUEST, i32::from(enabled))
}

pub(crate) fn set_inband_fec(encoder: &mut EncoderPtr, enabled: bool) -> Result<(), Status> {
    encoder_ctl_value(
        encoder,
        raw::OPUS_SET_INBAND_FEC_REQUEST,
        i32::from(enabled),
    )
}

pub(crate) fn set_expected_packet_loss(
    encoder: &mut EncoderPtr,
    percentage: i32,
) -> Result<(), Status> {
    encoder_ctl_value(encoder, raw::OPUS_SET_PACKET_LOSS_PERC_REQUEST, percentage)
}

pub(crate) fn set_dtx(encoder: &mut EncoderPtr, enabled: bool) -> Result<(), Status> {
    encoder_ctl_value(encoder, raw::OPUS_SET_DTX_REQUEST, i32::from(enabled))
}

pub(crate) fn encoder_lookahead(encoder: &mut EncoderPtr) -> Result<i32, Status> {
    let mut value = 0_i32;
    // SAFETY: the encoder is exclusively borrowed and `value` is the correctly
    // typed, uniquely writable argument required by this ctl request.
    let result = unsafe {
        raw::opus_encoder_ctl(
            encoder.0.as_ptr(),
            native_request(raw::OPUS_GET_LOOKAHEAD_REQUEST),
            &raw mut value,
        )
    };
    check(result)?;
    Ok(value)
}

pub(crate) fn reset_encoder(encoder: &mut EncoderPtr) -> Result<(), Status> {
    // SAFETY: the encoder is exclusively borrowed and RESET_STATE takes no
    // variadic payload argument.
    let result =
        unsafe { raw::opus_encoder_ctl(encoder.0.as_ptr(), native_request(raw::OPUS_RESET_STATE)) };
    check(result)
}

fn encoder_ctl_value(encoder: &mut EncoderPtr, request: u32, value: i32) -> Result<(), Status> {
    // SAFETY: callers select ctl requests whose variadic payload is exactly one
    // `int`, and the encoder is exclusively borrowed.
    let result =
        unsafe { raw::opus_encoder_ctl(encoder.0.as_ptr(), native_request(request), value) };
    check(result)
}

pub(crate) fn create_decoder(
    sample_rate: SampleRate,
    channels: Channels,
) -> Result<DecoderPtr, Status> {
    let mut error = native_request(raw::OPUS_OK);
    // SAFETY: all scalar parameters are validated typed values and `error` is a
    // uniquely writable out-parameter for the duration of the call.
    let pointer = unsafe {
        raw::opus_decoder_create(sample_rate.native(), channels.native(), &raw mut error)
    };
    check(error)?;
    NonNull::new(pointer)
        .map(DecoderPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_decoder(decoder: &mut DecoderPtr) {
    // SAFETY: the safe owner invokes this matching destructor once and never
    // exposes the pointer again afterwards.
    unsafe { raw::opus_decoder_destroy(decoder.0.as_ptr()) };
}

pub(crate) fn decode_float(
    decoder: &mut DecoderPtr,
    packet: Option<&[u8]>,
    output: &mut [f32],
    samples_per_channel: i32,
    decode_fec: bool,
) -> Result<usize, Status> {
    let (data, length) = match packet {
        Some(packet) => (packet.as_ptr(), native_length(packet.len())?),
        None => (std::ptr::null(), 0),
    };
    // SAFETY: the decoder is exclusively borrowed, `data` is null only for the
    // documented packet-loss path or readable for `length`, and `output` has
    // room for the requested samples across the decoder's channel count.
    let result = unsafe {
        raw::opus_decode_float(
            decoder.0.as_ptr(),
            data,
            length,
            output.as_mut_ptr(),
            samples_per_channel,
            i32::from(decode_fec),
        )
    };
    output_size(result)
}

pub(crate) fn reset_decoder(decoder: &mut DecoderPtr) -> Result<(), Status> {
    // SAFETY: the decoder is exclusively borrowed and RESET_STATE takes no
    // variadic payload argument.
    let result =
        unsafe { raw::opus_decoder_ctl(decoder.0.as_ptr(), native_request(raw::OPUS_RESET_STATE)) };
    check(result)
}

pub(crate) fn version_string() -> Result<&'static str, Status> {
    // SAFETY: Opus returns either null or a process-lifetime version string.
    let pointer = unsafe { raw::opus_get_version_string() };
    if pointer.is_null() {
        return Err(Status::ContractViolation);
    }
    // SAFETY: the non-null pointer above addresses Opus's NUL-terminated
    // process-lifetime version string.
    let version = unsafe { CStr::from_ptr(pointer) };
    version.to_str().map_err(|_error| Status::ContractViolation)
}

fn raw_application(application: Application) -> i32 {
    match application {
        Application::Voip => native_request(raw::OPUS_APPLICATION_VOIP),
        Application::Audio => native_request(raw::OPUS_APPLICATION_AUDIO),
        Application::RestrictedLowDelay => {
            native_request(raw::OPUS_APPLICATION_RESTRICTED_LOWDELAY)
        }
    }
}

fn native_request(request: u32) -> i32 {
    i32::try_from(request)
        .unwrap_or_else(|_error| unreachable!("Opus request values must fit a native C int"))
}

fn native_length(length: usize) -> Result<i32, Status> {
    i32::try_from(length).map_err(|_error| Status::BadArgument)
}

fn output_size(result: i32) -> Result<usize, Status> {
    if result < 0 {
        Err(status(result))
    } else {
        usize::try_from(result).map_err(|_error| Status::ContractViolation)
    }
}

fn check(result: i32) -> Result<(), Status> {
    if result == native_request(raw::OPUS_OK) {
        Ok(())
    } else {
        Err(status(result))
    }
}

fn status(result: i32) -> Status {
    match result {
        raw::OPUS_BAD_ARG => Status::BadArgument,
        raw::OPUS_BUFFER_TOO_SMALL => Status::BufferTooSmall,
        raw::OPUS_INTERNAL_ERROR => Status::Internal,
        raw::OPUS_INVALID_PACKET => Status::InvalidPacket,
        raw::OPUS_UNIMPLEMENTED => Status::Unimplemented,
        raw::OPUS_INVALID_STATE => Status::InvalidState,
        raw::OPUS_ALLOC_FAIL => Status::AllocationFailed,
        code => Status::Unknown(code),
    }
}
