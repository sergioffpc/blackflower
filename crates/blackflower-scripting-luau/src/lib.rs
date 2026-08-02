#![doc = include_str!("../README.md")]

mod compile;
mod config;
mod debug;
mod error;
mod ffi;
mod runtime;
mod value;

pub use compile::{
    Bytecode, CompileOptions, CoverageLevel, DebugLevel, OptimizationLevel, TypeInfoLevel, compile,
};
pub use config::{
    DEFAULT_EXECUTION_FUEL, DEFAULT_NATIVE_CODEGEN_LIMIT_BYTES, DEFAULT_VM_MEMORY_LIMIT_BYTES,
    Library, MIN_NATIVE_CODEGEN_LIMIT_BYTES, MemoryUsage, NativeCodegenStats, RuntimeConfig,
    SandboxPolicy,
};
pub use debug::{
    DebugAction, DebugEvent, DebugEventKind, DebugFrame, DebugHandler, DebugOptions, DebugValue,
    DebugVariable,
};
pub use error::Error;
pub use runtime::Runtime;
pub use value::Value;

/// The Luau version compiled into this crate.
#[must_use]
pub fn luau_version() -> (u32, u32, u32) {
    ffi::luau_version()
}

/// Whether the current target supports Luau native code generation.
#[must_use]
pub fn native_codegen_supported() -> bool {
    ffi::native_codegen_supported()
}
