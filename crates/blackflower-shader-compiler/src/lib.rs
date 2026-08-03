#![doc = include_str!("../README.md")]

mod compile;
mod error;
mod ffi;

pub use compile::{
    CompileOptions, DebugInfoLevel, OptimizationLevel, ShaderStage, compile, slang_version,
};
pub use error::Error;
