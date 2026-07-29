#![doc = include_str!("../README.md")]

mod compile;
mod error;
mod ffi;
mod runtime;
mod value;

pub use compile::{
    Bytecode, CompileOptions, CoverageLevel, DebugLevel, OptimizationLevel, TypeInfoLevel, compile,
};
pub use error::Error;
pub use runtime::Runtime;
pub use value::Value;

/// The Luau version compiled into this crate.
#[must_use]
pub fn luau_version() -> (u32, u32, u32) {
    ffi::luau_version()
}
