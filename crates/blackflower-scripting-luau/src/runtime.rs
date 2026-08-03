use std::ffi::CString;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::compile::{CompileOptions, VerifiedBytecode};
use crate::ffi::DebugRequest;
use crate::{
    DebugHandler, DebugOptions, Error, MIN_NATIVE_CODEGEN_LIMIT_BYTES, MemoryUsage,
    NativeCodegenStats, RuntimeConfig, Value, compile, ffi,
};

/// An isolated Luau VM with a fresh, deterministically seeded environment per evaluation.
///
/// `os`, `debug`, filesystem, networking, and module loading are not exposed.
/// VM allocations and execution safepoints are bounded by [`RuntimeConfig`].
/// Fuel is not a wall-clock deadline: hostile execution must be supervised by
/// a killable worker outside the deterministic simulation tick.
/// The VM is neither [`Send`] nor [`Sync`]; its owner must keep it on one
/// execution thread.
pub struct Runtime {
    state: ffi::State,
    config: RuntimeConfig,
    last_native_codegen_stats: Option<NativeCodegenStats>,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl Runtime {
    /// Create a sandboxed runtime with finite default resource limits.
    pub fn new() -> Result<Self, Error> {
        Self::with_config(RuntimeConfig::default())
    }

    /// Create a sandboxed runtime with an explicit default per-evaluation
    /// `math.random` seed and otherwise default resource limits.
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
        let native_codegen_limit = config.native_codegen_limit_bytes();
        if native_codegen_limit != 0 && native_codegen_limit < MIN_NATIVE_CODEGEN_LIMIT_BYTES {
            return Err(Error::InvalidNativeCodegenLimit);
        }

        Ok(Self {
            state: ffi::State::new(config)?,
            config,
            last_native_codegen_stats: None,
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

    /// Current and peak executable-memory usage owned by native codegen.
    #[must_use]
    pub fn native_codegen_memory_usage(&self) -> MemoryUsage {
        self.state.native_codegen_memory_usage()
    }

    /// Native compilation statistics from the most recent successful execution.
    #[must_use]
    pub const fn last_native_codegen_stats(&self) -> Option<NativeCodegenStats> {
        self.last_native_codegen_stats
    }

    /// Compile and execute one chunk with Luau's baseline compiler options.
    pub fn execute(&mut self, chunk_name: &str, source: &str) -> Result<Vec<Value>, Error> {
        self.execute_with_options(chunk_name, source, CompileOptions::default())
    }

    /// Compile and execute one chunk with a host-derived evaluation seed.
    pub fn execute_seeded(
        &mut self,
        chunk_name: &str,
        source: &str,
        random_seed: i32,
    ) -> Result<Vec<Value>, Error> {
        self.execute_with_options_seeded(chunk_name, source, CompileOptions::default(), random_seed)
    }

    /// Compile and execute one chunk with explicit compiler options.
    pub fn execute_with_options(
        &mut self,
        chunk_name: &str,
        source: &str,
        options: CompileOptions,
    ) -> Result<Vec<Value>, Error> {
        self.execute_with_options_seeded(chunk_name, source, options, self.config.random_seed())
    }

    /// Compile and execute one seeded chunk with explicit compiler options.
    pub fn execute_with_options_seeded(
        &mut self,
        chunk_name: &str,
        source: &str,
        options: CompileOptions,
        random_seed: i32,
    ) -> Result<Vec<Value>, Error> {
        let bytecode = compile(source, options)?;
        self.execute_bytecode_seeded(chunk_name, &bytecode, random_seed)
    }

    /// Compile and execute one chunk under the host-controlled debugger.
    ///
    /// Use [`crate::DebugLevel::Full`] when the handler needs named locals and
    /// upvalues. [`crate::DebugLevel::LineInfo`] is sufficient for source
    /// breakpoints, stepping, and stack-frame locations.
    pub fn execute_with_options_debugged(
        &mut self,
        chunk_name: &str,
        source: &str,
        compile_options: CompileOptions,
        debug_options: &DebugOptions,
        handler: &mut dyn DebugHandler,
    ) -> Result<Vec<Value>, Error> {
        let bytecode = compile(source, compile_options)?;
        self.execute_bytecode_inner(
            chunk_name,
            &bytecode,
            self.config.random_seed(),
            Some(DebugRequest {
                options: debug_options,
                handler,
            }),
        )
    }

    /// Load and execute bytecode produced for the pinned Luau version.
    pub fn execute_bytecode(
        &mut self,
        chunk_name: &str,
        bytecode: &VerifiedBytecode,
    ) -> Result<Vec<Value>, Error> {
        self.execute_bytecode_seeded(chunk_name, bytecode, self.config.random_seed())
    }

    /// Execute verified bytecode with a host-derived evaluation seed.
    pub fn execute_bytecode_seeded(
        &mut self,
        chunk_name: &str,
        bytecode: &VerifiedBytecode,
        random_seed: i32,
    ) -> Result<Vec<Value>, Error> {
        self.execute_bytecode_inner(chunk_name, bytecode, random_seed, None)
    }

    /// Execute bytecode with host-controlled breakpoints and single stepping.
    ///
    /// Native execution is suspended for this call because Luau 0.731 does
    /// not support debugging native frames.
    pub fn execute_bytecode_debugged(
        &mut self,
        chunk_name: &str,
        bytecode: &VerifiedBytecode,
        options: &DebugOptions,
        handler: &mut dyn DebugHandler,
    ) -> Result<Vec<Value>, Error> {
        self.execute_bytecode_inner(
            chunk_name,
            bytecode,
            self.config.random_seed(),
            Some(DebugRequest { options, handler }),
        )
    }

    fn execute_bytecode_inner(
        &mut self,
        chunk_name: &str,
        bytecode: &VerifiedBytecode,
        random_seed: i32,
        debug: Option<DebugRequest<'_>>,
    ) -> Result<Vec<Value>, Error> {
        let chunk_name =
            CString::new(format!("={chunk_name}")).map_err(|_error| Error::InvalidChunkName)?;
        self.last_native_codegen_stats = None;
        let (values, stats) = self.state.execute(
            &chunk_name,
            bytecode.as_bytes(),
            bytecode.compile_options(),
            self.config.execution_fuel(),
            random_seed,
            debug,
        )?;
        self.last_native_codegen_stats = stats;
        Ok(values)
    }
}
