#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw Luau calls and native pointer materialization are isolated in this private module"
)]
#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block,
    reason = "all unsafe operations are confined to the reviewed Luau FFI boundary"
)]

use std::ffi::{CStr, CString};
use std::ptr::NonNull;
use std::slice;

use crate::compile::CompileOptions;
use crate::{Error, Library, MemoryUsage, RuntimeConfig, SandboxPolicy, Value};

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the Luau and blackflower C APIs"
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
pub(crate) mod raw {
    include!(concat!(env!("OUT_DIR"), "/luau_bindings.rs"));
}

trait MatchesCInt {
    fn matches_c_int(self, value: i32) -> bool;
}

impl MatchesCInt for i32 {
    fn matches_c_int(self, value: i32) -> bool {
        self == value
    }
}

impl MatchesCInt for u32 {
    fn matches_c_int(self, value: i32) -> bool {
        u32::try_from(value).is_ok_and(|value| self == value)
    }
}

pub(crate) fn luau_version() -> (u32, u32, u32) {
    let version = unsafe { raw::bf_scripting_luau_version() };
    (version.major, version.minor, version.patch)
}

pub(crate) fn compile(source: &str, options: CompileOptions) -> Result<Vec<u8>, Error> {
    let options = raw::BFScriptingCompileOptions {
        optimization_level: options.optimization as i32,
        debug_level: options.debug as i32,
        type_info_level: options.type_info as i32,
        coverage_level: options.coverage as i32,
    };
    let mut bytecode = raw::BFScriptingBytecode::default();
    let status = unsafe {
        raw::bf_scripting_compile(
            source.as_ptr().cast(),
            source.len(),
            &raw const options,
            &raw mut bytecode,
        )
    };
    check_compile_status(status)?;

    let pointer = NonNull::new(bytecode.data).ok_or(Error::NativeContract)?;
    if bytecode.size == 0 {
        unsafe { raw::bf_scripting_bytecode_free(pointer.as_ptr().cast()) };
        return Err(Error::NativeContract);
    }
    let bytes = unsafe { slice::from_raw_parts(pointer.as_ptr(), bytecode.size) }.to_vec();
    unsafe { raw::bf_scripting_bytecode_free(pointer.as_ptr().cast()) };
    Ok(bytes)
}

fn sandbox_library_mask(policy: SandboxPolicy) -> u32 {
    Library::ALL
        .into_iter()
        .filter_map(|library| policy.allows(library).then_some(raw_library_mask(library)))
        .fold(0, |libraries, library| libraries | library)
}

const fn raw_library_mask(library: Library) -> u32 {
    match library {
        Library::Base => raw::BF_SCRIPTING_LIBRARY_BASE,
        Library::Coroutine => raw::BF_SCRIPTING_LIBRARY_COROUTINE,
        Library::Table => raw::BF_SCRIPTING_LIBRARY_TABLE,
        Library::String => raw::BF_SCRIPTING_LIBRARY_STRING,
        Library::Math => raw::BF_SCRIPTING_LIBRARY_MATH,
        Library::Utf8 => raw::BF_SCRIPTING_LIBRARY_UTF8,
        Library::Bit32 => raw::BF_SCRIPTING_LIBRARY_BIT32,
        Library::Buffer => raw::BF_SCRIPTING_LIBRARY_BUFFER,
        Library::Vector => raw::BF_SCRIPTING_LIBRARY_VECTOR,
        Library::Integer => raw::BF_SCRIPTING_LIBRARY_INTEGER,
    }
}

fn check_compile_status(status: i32) -> Result<(), Error> {
    match status {
        value if raw::BF_SCRIPTING_STATUS_OK.matches_c_int(value) => Ok(()),
        value if raw::BF_SCRIPTING_STATUS_OUT_OF_MEMORY.matches_c_int(value) => {
            Err(Error::OutOfMemory)
        }
        value if raw::BF_SCRIPTING_STATUS_COMPILER_FAILED.matches_c_int(value) => {
            Err(Error::CompilerFailure)
        }
        value
            if raw::BF_SCRIPTING_STATUS_NULL_POINTER.matches_c_int(value)
                || raw::BF_SCRIPTING_STATUS_INVALID_ARGUMENT.matches_c_int(value) =>
        {
            Err(Error::NativeContract)
        }
        _ => Err(Error::NativeContract),
    }
}

pub(crate) struct State(raw::BFScriptingRuntime);

impl State {
    pub(crate) fn new(config: RuntimeConfig) -> Result<Self, Error> {
        let mut runtime = raw::BFScriptingRuntime::default();
        let creation_status = unsafe {
            raw::bf_scripting_runtime_new(config.vm_memory_limit_bytes(), &raw mut runtime)
        };
        if raw::BF_SCRIPTING_STATUS_OUT_OF_MEMORY.matches_c_int(creation_status) {
            return Err(Error::OutOfMemory);
        }
        if !raw::BF_SCRIPTING_STATUS_OK.matches_c_int(creation_status) {
            return Err(Error::NativeContract);
        }

        let state = Self(runtime);
        if NonNull::new(state.pointer()).is_none() {
            return Err(Error::NativeContract);
        }
        let status = unsafe {
            raw::bf_scripting_initialize(
                state.pointer(),
                config.random_seed(),
                sandbox_library_mask(config.sandbox_policy()),
            )
        };
        if raw::lua_Status_LUA_OK.matches_c_int(status) {
            return Ok(state);
        }

        let message = state.error_message(-1);
        if raw::lua_Status_LUA_ERRMEM.matches_c_int(status) {
            Err(Error::OutOfMemory)
        } else {
            Err(Error::Initialization(message))
        }
    }

    pub(crate) fn execute(
        &mut self,
        chunk_name: &CString,
        bytecode: &[u8],
        execution_fuel: u64,
    ) -> Result<Vec<Value>, Error> {
        let base = unsafe { raw::lua_gettop(self.pointer()) };
        let load_status = unsafe {
            raw::luau_load(
                self.pointer(),
                chunk_name.as_ptr(),
                bytecode.as_ptr().cast(),
                bytecode.len(),
                0,
            )
        };
        if !raw::lua_Status_LUA_OK.matches_c_int(load_status) {
            let error = if raw::lua_Status_LUA_ERRMEM.matches_c_int(load_status) {
                Error::OutOfMemory
            } else {
                Error::Compile(self.error_message(-1))
            };
            unsafe { raw::lua_settop(self.pointer(), base) };
            return Err(error);
        }

        let begin_status =
            unsafe { raw::bf_scripting_begin_execution(self.pointer(), execution_fuel) };
        if !raw::BF_SCRIPTING_STATUS_OK.matches_c_int(begin_status) {
            unsafe { raw::lua_settop(self.pointer(), base) };
            return Err(Error::NativeContract);
        }

        let call_status = unsafe { raw::lua_pcall(self.pointer(), 0, raw::LUA_MULTRET, 0) };
        let end_status = unsafe { raw::bf_scripting_end_execution(self.pointer()) };
        if raw::BF_SCRIPTING_STATUS_EXECUTION_LIMIT.matches_c_int(end_status) {
            unsafe { raw::lua_settop(self.pointer(), base) };
            return Err(Error::ExecutionLimit);
        }
        if !raw::BF_SCRIPTING_STATUS_OK.matches_c_int(end_status) {
            unsafe { raw::lua_settop(self.pointer(), base) };
            return Err(Error::NativeContract);
        }
        if !raw::lua_Status_LUA_OK.matches_c_int(call_status) {
            let error = if raw::lua_Status_LUA_ERRMEM.matches_c_int(call_status) {
                Error::OutOfMemory
            } else {
                Error::Runtime(self.error_message(-1))
            };
            unsafe { raw::lua_settop(self.pointer(), base) };
            return Err(error);
        }

        let results = self.collect_results(base);
        unsafe { raw::lua_settop(self.pointer(), base) };
        results
    }

    pub(crate) fn memory_usage(&self) -> MemoryUsage {
        let usage = unsafe { raw::bf_scripting_runtime_memory_usage(&raw const self.0) };
        MemoryUsage {
            current_bytes: usage.current_bytes,
            peak_bytes: usage.peak_bytes,
            limit_bytes: usage.limit_bytes,
        }
    }

    fn collect_results(&self, base: i32) -> Result<Vec<Value>, Error> {
        let top = unsafe { raw::lua_gettop(self.pointer()) };
        let count = top.checked_sub(base).ok_or(Error::NativeContract)?;
        let capacity = usize::try_from(count).map_err(|_error| Error::NativeContract)?;
        let mut results = Vec::with_capacity(capacity);
        for offset in 0..count {
            let stack_index = base
                .checked_add(offset)
                .and_then(|index| index.checked_add(1))
                .ok_or(Error::NativeContract)?;
            let result_index = usize::try_from(offset).map_err(|_error| Error::NativeContract)?;
            results.push(self.read_value(stack_index, result_index)?);
        }
        Ok(results)
    }

    fn read_value(&self, stack_index: i32, result_index: usize) -> Result<Value, Error> {
        let value_type = unsafe { raw::lua_type(self.pointer(), stack_index) };
        match value_type {
            value if raw::lua_Type_LUA_TNIL.matches_c_int(value) => Ok(Value::Nil),
            value if raw::lua_Type_LUA_TBOOLEAN.matches_c_int(value) => Ok(Value::Boolean(
                unsafe { raw::lua_toboolean(self.pointer(), stack_index) } != 0,
            )),
            value if raw::lua_Type_LUA_TNUMBER.matches_c_int(value) => Ok(Value::Number(unsafe {
                raw::lua_tonumberx(self.pointer(), stack_index, std::ptr::null_mut())
            })),
            value if raw::lua_Type_LUA_TINTEGER.matches_c_int(value) => {
                let mut is_integer = 0;
                let integer = unsafe {
                    raw::lua_tointeger64(self.pointer(), stack_index, &raw mut is_integer)
                };
                if is_integer == 0 {
                    Err(Error::NativeContract)
                } else {
                    Ok(Value::Integer(integer))
                }
            }
            value if raw::lua_Type_LUA_TSTRING.matches_c_int(value) => {
                self.read_string(stack_index).map(Value::String)
            }
            value if raw::lua_Type_LUA_TVECTOR.matches_c_int(value) => {
                let pointer = NonNull::new(
                    unsafe { raw::lua_tovector(self.pointer(), stack_index) }.cast_mut(),
                )
                .ok_or(Error::NativeContract)?;
                let components = unsafe { slice::from_raw_parts(pointer.as_ptr(), 3) };
                Ok(Value::Vector([components[0], components[1], components[2]]))
            }
            _ => Err(Error::UnsupportedValue {
                index: result_index,
                type_name: self.type_name(value_type),
            }),
        }
    }

    fn read_string(&self, stack_index: i32) -> Result<Box<[u8]>, Error> {
        let mut length = 0;
        let pointer = NonNull::new(
            unsafe { raw::lua_tolstring(self.pointer(), stack_index, &raw mut length) }.cast_mut(),
        )
        .ok_or(Error::NativeContract)?;
        Ok(
            unsafe { slice::from_raw_parts(pointer.as_ptr().cast(), length) }
                .to_vec()
                .into_boxed_slice(),
        )
    }

    fn type_name(&self, value_type: i32) -> String {
        let pointer = unsafe { raw::lua_typename(self.pointer(), value_type) };
        if pointer.is_null() {
            return "unknown".to_owned();
        }
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }

    fn error_message(&self, stack_index: i32) -> String {
        let mut length = 0;
        let pointer = unsafe { raw::lua_tolstring(self.pointer(), stack_index, &raw mut length) };
        if pointer.is_null() {
            return "unknown native error".to_owned();
        }
        let bytes = unsafe { slice::from_raw_parts(pointer.cast(), length) };
        String::from_utf8_lossy(bytes).into_owned()
    }

    const fn pointer(&self) -> *mut raw::lua_State {
        self.0.state
    }
}

impl Drop for State {
    fn drop(&mut self) {
        unsafe { raw::bf_scripting_runtime_free(&raw mut self.0) };
    }
}
