#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw KTX-Software calls and pointer materialization are isolated in this private module"
)]
use std::ffi::CStr;
use std::slice;

use bytes::Bytes;

use crate::Error;
use crate::texture::{
    EncodeOptions, TextureFormat, TextureMip, TextureSemantic, TranscodedMip, TranscodedTexture,
};

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the Blackflower KTX C API"
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
#[allow(
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks,
    reason = "bindgen output is generated from the pinned KTX-Software headers"
)]
mod raw {
    include!(concat!(env!("OUT_DIR"), "/ktx_bindings.rs"));
}

pub(crate) struct NativeInfo {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) levels: u32,
    pub(crate) semantic: TextureSemantic,
}

pub(crate) fn encode(
    levels: &[TextureMip<'_>],
    semantic: TextureSemantic,
    options: EncodeOptions,
) -> Result<Bytes, Error> {
    let native_levels = levels
        .iter()
        .map(|level| raw::BFTextureSourceLevel {
            data: level.bytes.as_ptr(),
            size: level.bytes.len(),
            width: level.width,
            height: level.height,
        })
        .collect::<Vec<_>>();
    let native_options = raw::BFTextureEncodeOptions {
        semantic: semantic.raw(),
        quality: options.quality.raw(),
        zstd_level: options.zstd_level,
        uastc_rdo: u8::from(options.uastc_rdo),
    };
    let mut output = OwnedBlob::default();
    // SAFETY: the level table and every referenced mip remain alive for the
    // call, their lengths are explicit, and `output` is uniquely writable.
    let status = unsafe {
        raw::bf_texture_encode(
            native_levels.as_ptr(),
            native_levels.len(),
            &raw const native_options,
            &raw mut output.0,
        )
    };
    status_result(status)?;
    Ok(Bytes::copy_from_slice(output.bytes()))
}

pub(crate) fn inspect(bytes: &[u8]) -> Result<NativeInfo, Error> {
    let mut info = raw::BFTextureInfo::default();
    // SAFETY: `bytes` is readable for its supplied length and `info` is a valid
    // writable output record for the duration of the call.
    let status = unsafe { raw::bf_texture_inspect(bytes.as_ptr(), bytes.len(), &raw mut info) };
    status_result(status)?;
    Ok(NativeInfo {
        width: info.width,
        height: info.height,
        levels: info.levels,
        semantic: TextureSemantic::from_raw(info.semantic)?,
    })
}

pub(crate) fn transcode(bytes: &[u8], format: TextureFormat) -> Result<TranscodedTexture, Error> {
    let mut native = raw::BFTranscodedTexture::default();
    // SAFETY: the encoded input is readable for its supplied length and the
    // uniquely owned native output record is writable.
    let status = unsafe {
        raw::bf_texture_transcode(bytes.as_ptr(), bytes.len(), format.raw(), &raw mut native)
    };
    status_result(status)?;

    let level_count = usize::try_from(native.level_count)
        .map_err(|_error| Error::InvalidKtx2("mip count does not fit usize".to_owned()))?;
    let layouts = native.levels.get(..level_count).ok_or_else(|| {
        Error::InvalidKtx2("native mip count exceeds its layout array".to_owned())
    })?;
    let levels = layouts
        .iter()
        .map(|level| TranscodedMip {
            width: level.width,
            height: level.height,
            offset: level.offset,
            byte_len: level.size,
        })
        .collect();
    let owned = OwnedBlob(native.bytes);
    let output = TranscodedTexture {
        format: TextureFormat::from_raw(native.format)?,
        semantic: TextureSemantic::from_raw(native.semantic)?,
        width: native.width,
        height: native.height,
        levels,
        bytes: Bytes::copy_from_slice(owned.bytes()),
    };
    Ok(output)
}

pub(crate) fn ktx_version() -> &'static str {
    // SAFETY: the wrapper returns either null or a process-lifetime KTX version string.
    let pointer = unsafe { raw::bf_texture_ktx_version() };
    if pointer.is_null() {
        return "unknown";
    }
    // SAFETY: the non-null pointer above addresses the wrapper's NUL-terminated
    // process-lifetime version string.
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .unwrap_or("unknown")
}

fn status_result(status: i32) -> Result<(), Error> {
    if status == raw::BF_TEXTURE_STATUS_OK.cast_signed() {
        return Ok(());
    }
    // SAFETY: the wrapper accepts every status value and returns either null or
    // a process-lifetime diagnostic string.
    let pointer = unsafe { raw::bf_texture_status_message(status) };
    let detail = if pointer.is_null() {
        format!("native status {status}")
    } else {
        // SAFETY: the non-null pointer above addresses a NUL-terminated
        // process-lifetime diagnostic string owned by the wrapper.
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    };
    if status == raw::BF_TEXTURE_STATUS_INVALID_ARGUMENT.cast_signed()
        || status == raw::BF_TEXTURE_STATUS_NULL_POINTER.cast_signed()
    {
        Err(Error::InvalidInput(detail))
    } else if status == raw::BF_TEXTURE_STATUS_INVALID_KTX2.cast_signed() {
        Err(Error::InvalidKtx2(detail))
    } else if status == raw::BF_TEXTURE_STATUS_UNSUPPORTED.cast_signed() {
        Err(Error::Unsupported(detail))
    } else {
        Err(Error::Native(detail))
    }
}

#[derive(Default)]
struct OwnedBlob(raw::BFTextureBlob);

impl OwnedBlob {
    fn bytes(&self) -> &[u8] {
        if self.0.data.is_null() || self.0.size == 0 {
            return &[];
        }
        // SAFETY: a successful native blob owns `size` readable bytes until
        // this `OwnedBlob` releases it.
        unsafe { slice::from_raw_parts(self.0.data, self.0.size) }
    }
}

impl Drop for OwnedBlob {
    fn drop(&mut self) {
        if self.0.data.is_null() {
            return;
        }
        // SAFETY: the blob pointer was allocated by the wrapper, is uniquely
        // owned by this value, and is released exactly once here.
        unsafe {
            raw::bf_texture_blob_free(self.0.data.cast());
        }
        self.0.data = core::ptr::null_mut();
        self.0.size = 0;
    }
}
