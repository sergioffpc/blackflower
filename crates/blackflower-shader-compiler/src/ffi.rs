#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw Slang calls and native pointer materialization are isolated in this private module"
)]
use std::ffi::CStr;
use std::slice;

use bytes::Bytes;

use crate::Error;
use crate::compile::CompileOptions;

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the Blackflower shader compiler C API"
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
    reason = "bindgen output is generated from the pinned Slang wrapper headers"
)]
mod raw {
    include!(concat!(env!("OUT_DIR"), "/slang_bindings.rs"));
}

pub(crate) fn compile(
    source_name: &str,
    source: &str,
    entry_point: &str,
    options: CompileOptions,
) -> Result<Bytes, Error> {
    let native_options = raw::BFShaderCompilerOptions {
        stage: options.stage as i32,
        optimization: options.optimization as i32,
        debug_info: options.debug_info as i32,
    };
    let mut spirv = OwnedBlob::default();
    let mut diagnostics = OwnedBlob::default();
    // SAFETY: all string pointers remain valid for their explicit byte lengths
    // during the call, and the two distinct output records are writable.
    let status = unsafe {
        raw::bf_shader_compiler_compile_spirv(
            source_name.as_ptr(),
            source_name.len(),
            source.as_ptr(),
            source.len(),
            entry_point.as_ptr(),
            entry_point.len(),
            &native_options,
            &mut spirv.0,
            &mut diagnostics.0,
        )
    };
    let diagnostic_text = diagnostics.text();
    if status != raw::BF_SHADER_COMPILER_STATUS_OK.cast_signed() {
        return Err(status_error(status, diagnostic_text));
    }
    Ok(Bytes::copy_from_slice(spirv.bytes()))
}

pub(crate) fn slang_version() -> &'static str {
    // SAFETY: the wrapper returns either null or a process-lifetime Slang version string.
    let pointer = unsafe { raw::bf_shader_compiler_slang_version() };
    if pointer.is_null() {
        return "unknown";
    }
    // SAFETY: the non-null pointer above addresses the wrapper's NUL-terminated
    // process-lifetime version string.
    let value = unsafe { CStr::from_ptr(pointer) };
    value.to_str().unwrap_or("unknown")
}

fn status_error(status: i32, diagnostics: String) -> Error {
    let detail = if diagnostics.is_empty() {
        format!("native status {status}")
    } else {
        diagnostics
    };
    if status == raw::BF_SHADER_COMPILER_STATUS_INVALID_ARGUMENT.cast_signed()
        || status == raw::BF_SHADER_COMPILER_STATUS_NULL_POINTER.cast_signed()
    {
        Error::InvalidInput(detail)
    } else if status == raw::BF_SHADER_COMPILER_STATUS_INITIALIZATION_FAILED.cast_signed() {
        Error::Initialization(detail)
    } else if status == raw::BF_SHADER_COMPILER_STATUS_COMPILATION_FAILED.cast_signed() {
        Error::Compilation(detail)
    } else if status == raw::BF_SHADER_COMPILER_STATUS_OUT_OF_MEMORY.cast_signed() {
        Error::InvalidOutput("native compiler allocation failed".to_owned())
    } else {
        Error::InvalidOutput(detail)
    }
}

#[derive(Default)]
struct OwnedBlob(raw::BFShaderCompilerBlob);

impl OwnedBlob {
    fn bytes(&self) -> &[u8] {
        if self.0.data.is_null() || self.0.size == 0 {
            return &[];
        }
        // SAFETY: a successful native blob owns `size` readable bytes and keeps
        // them alive until this `OwnedBlob` is dropped.
        unsafe { slice::from_raw_parts(self.0.data, self.0.size) }
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(self.bytes()).trim().to_owned()
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
            raw::bf_shader_compiler_blob_free(self.0.data.cast());
        }
        self.0.data = core::ptr::null_mut();
        self.0.size = 0;
    }
}
