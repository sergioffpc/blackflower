use std::ffi::CString;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::compile::{Bytecode, CompileOptions};
use crate::{Error, Value, compile, ffi};

/// An isolated Luau VM with deterministic standard-library initialization.
///
/// `os`, `debug`, filesystem, networking, and module loading are not exposed.
/// The VM is neither [`Send`] nor [`Sync`]; its owner must keep it on one
/// execution thread.
pub struct Runtime {
    state: ffi::State,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl Runtime {
    /// Create a sandboxed runtime with RNG seed zero.
    pub fn new() -> Result<Self, Error> {
        Self::with_seed(0)
    }

    /// Create a sandboxed runtime with an explicit `math.random` seed.
    pub fn with_seed(random_seed: i32) -> Result<Self, Error> {
        Ok(Self {
            state: ffi::State::new(random_seed)?,
            not_send_or_sync: PhantomData,
        })
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
        self.state.execute(&chunk_name, bytecode.as_bytes())
    }
}
