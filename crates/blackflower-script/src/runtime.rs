use std::ffi::CString;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::compile::{Bytecode, CompileOptions};
use crate::{Error, MemoryUsage, RuntimeConfig, Value, compile, ffi};

/// An isolated Luau VM with deterministic standard-library initialization.
///
/// `os`, `debug`, filesystem, networking, and module loading are not exposed.
/// VM allocations and execution safepoints are bounded by [`RuntimeConfig`].
/// The VM is neither [`Send`] nor [`Sync`]; its owner must keep it on one
/// execution thread.
pub struct Runtime {
    state: ffi::State,
    config: RuntimeConfig,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl Runtime {
    /// Create a sandboxed runtime with finite default resource limits.
    pub fn new() -> Result<Self, Error> {
        Self::with_config(RuntimeConfig::default())
    }

    /// Create a sandboxed runtime with an explicit `math.random` seed and
    /// otherwise default resource limits.
    pub fn with_seed(random_seed: i32) -> Result<Self, Error> {
        Self::with_config(RuntimeConfig::default().with_random_seed(random_seed))
    }

    /// Create a sandboxed runtime with explicit limits and library policy.
    pub fn with_config(config: RuntimeConfig) -> Result<Self, Error> {
        if config.vm_memory_limit_bytes() == 0 {
            return Err(Error::InvalidMemoryLimit);
        }
        if config.execution_fuel() == 0 {
            return Err(Error::InvalidExecutionFuel);
        }

        Ok(Self {
            state: ffi::State::new(config)?,
            config,
            not_send_or_sync: PhantomData,
        })
    }

    /// Configuration fixed when this runtime was created.
    #[must_use]
    pub const fn config(&self) -> RuntimeConfig {
        self.config
    }

    /// Current and peak VM allocator usage.
    #[must_use]
    pub fn memory_usage(&self) -> MemoryUsage {
        self.state.memory_usage()
    }

    /// Compile and execute one chunk with Luau's baseline compiler options.
    pub fn execute(&mut self, chunk_name: &str, source: &str) -> Result<Vec<Value>, Error> {
        self.execute_with_options(chunk_name, source, CompileOptions::default())
    }

    /// Compile and execute one chunk with explicit compiler options.
    pub fn execute_with_options(
        &mut self,
        chunk_name: &str,
        source: &str,
        options: CompileOptions,
    ) -> Result<Vec<Value>, Error> {
        let bytecode = compile(source, options)?;
        self.execute_bytecode(chunk_name, &bytecode)
    }

    /// Load and execute bytecode produced for the pinned Luau version.
    pub fn execute_bytecode(
        &mut self,
        chunk_name: &str,
        bytecode: &Bytecode,
    ) -> Result<Vec<Value>, Error> {
        let chunk_name = CString::new(chunk_name).map_err(|_error| Error::InvalidChunkName)?;
        self.state.execute(
            &chunk_name,
            bytecode.as_bytes(),
            self.config.execution_fuel(),
        )
    }
}
