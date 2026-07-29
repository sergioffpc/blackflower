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
use crate::{Error, Value};

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

pub(crate) fn luau_version() -> (u32, u32, u32) {
    let version = unsafe { raw::bf_script_luau_version() };
    (version.major, version.minor, version.patch)
}

pub(crate) fn compile(source: &str, options: CompileOptions) -> Result<Vec<u8>, Error> {
    let options = raw::BFScriptCompileOptions {
        optimization_level: options.optimization as i32,
        debug_level: options.debug as i32,
        type_info_level: options.type_info as i32,
        coverage_level: options.coverage as i32,
    };
    let mut bytecode = raw::BFScriptBytecode::default();
    let status = unsafe {
        raw::bf_script_compile(
            source.as_ptr().cast(),
            source.len(),
            &raw const options,
            &raw mut bytecode,
        )
    };
    check_compile_status(status)?;

    let pointer = NonNull::new(bytecode.data).ok_or(Error::NativeContract)?;
    if bytecode.size == 0 {
        unsafe { raw::bf_script_bytecode_free(pointer.as_ptr().cast()) };
        return Err(Error::NativeContract);
    }
    let bytes = unsafe { slice::from_raw_parts(pointer.as_ptr(), bytecode.size) }.to_vec();
    unsafe { raw::bf_script_bytecode_free(pointer.as_ptr().cast()) };
    Ok(bytes)
}

fn check_compile_status(status: i32) -> Result<(), Error> {
    match status {
        value if value == raw::BF_SCRIPT_STATUS_OK.cast_signed() => Ok(()),
        value if value == raw::BF_SCRIPT_STATUS_OUT_OF_MEMORY.cast_signed() => {
            Err(Error::OutOfMemory)
        }
        value if value == raw::BF_SCRIPT_STATUS_COMPILER_FAILED.cast_signed() => {
            Err(Error::CompilerFailure)
        }
        value
            if value == raw::BF_SCRIPT_STATUS_NULL_POINTER.cast_signed()
                || value == raw::BF_SCRIPT_STATUS_INVALID_ARGUMENT.cast_signed() =>
        {
            Err(Error::NativeContract)
        }
        _ => Err(Error::NativeContract),
    }
}

pub(crate) struct State(NonNull<raw::lua_State>);

impl State {
    pub(crate) fn new(random_seed: i32) -> Result<Self, Error> {
        let pointer = NonNull::new(unsafe { raw::luaL_newstate() }).ok_or(Error::OutOfMemory)?;
        let state = Self(pointer);
        let status = unsafe { raw::bf_script_initialize(state.pointer(), random_seed) };
        if status == raw::lua_Status_LUA_OK.cast_signed() {
            return Ok(state);
        }

        let message = state.error_message(-1);
        if status == raw::lua_Status_LUA_ERRMEM.cast_signed() {
            Err(Error::OutOfMemory)
        } else {
            Err(Error::Initialization(message))
        }
    }

    pub(crate) fn execute(
        &mut self,
        chunk_name: &CString,
        bytecode: &[u8],
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
        if load_status != raw::lua_Status_LUA_OK.cast_signed() {
            let error = if load_status == raw::lua_Status_LUA_ERRMEM.cast_signed() {
                Error::OutOfMemory
            } else {
                Error::Compile(self.error_message(-1))
            };
            unsafe { raw::lua_settop(self.pointer(), base) };
            return Err(error);
        }

        let call_status = unsafe { raw::lua_pcall(self.pointer(), 0, raw::LUA_MULTRET, 0) };
        if call_status != raw::lua_Status_LUA_OK.cast_signed() {
            let error = if call_status == raw::lua_Status_LUA_ERRMEM.cast_signed() {
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
            value if value == raw::lua_Type_LUA_TNIL.cast_signed() => Ok(Value::Nil),
            value if value == raw::lua_Type_LUA_TBOOLEAN.cast_signed() => Ok(Value::Boolean(
                unsafe { raw::lua_toboolean(self.pointer(), stack_index) } != 0,
            )),
            value if value == raw::lua_Type_LUA_TNUMBER.cast_signed() => {
                Ok(Value::Number(unsafe {
                    raw::lua_tonumberx(self.pointer(), stack_index, std::ptr::null_mut())
                }))
            }
            value if value == raw::lua_Type_LUA_TINTEGER.cast_signed() => {
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
            value if value == raw::lua_Type_LUA_TSTRING.cast_signed() => {
                self.read_string(stack_index).map(Value::String)
            }
            value if value == raw::lua_Type_LUA_TVECTOR.cast_signed() => {
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
        self.0.as_ptr()
    }
}

impl Drop for State {
    fn drop(&mut self) {
        unsafe { raw::lua_close(self.pointer()) };
    }
}
