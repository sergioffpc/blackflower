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

use std::ffi::{CStr, CString, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::slice;

use crate::compile::{CompileOptions, TypeInfoLevel};
use crate::{
    DebugAction, DebugEvent, DebugEventKind, DebugFrame, DebugHandler, DebugOptions, DebugValue,
    DebugVariable, Error, Library, MemoryUsage, NativeCodegenStats, RuntimeConfig, SandboxPolicy,
    Value,
};

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
    clippy::allow_attributes_without_reason,
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

pub(crate) fn native_codegen_supported() -> bool {
    unsafe { raw::bf_scripting_native_codegen_supported() != 0 }
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

pub(crate) struct DebugRequest<'a> {
    pub(crate) options: &'a DebugOptions,
    pub(crate) handler: &'a mut dyn DebugHandler,
}

struct DebugBridge<'a> {
    handler: &'a mut dyn DebugHandler,
    panicked: bool,
}

pub(crate) struct State(raw::BFScriptingRuntime);

impl State {
    pub(crate) fn new(config: RuntimeConfig) -> Result<Self, Error> {
        let mut runtime = raw::BFScriptingRuntime::default();
        let creation_status = unsafe {
            raw::bf_scripting_runtime_new(
                config.vm_memory_limit_bytes(),
                config.native_codegen_limit_bytes(),
                &raw mut runtime,
            )
        };
        if raw::BF_SCRIPTING_STATUS_OUT_OF_MEMORY.matches_c_int(creation_status) {
            return Err(Error::OutOfMemory);
        }
        if raw::BF_SCRIPTING_STATUS_CODEGEN_UNSUPPORTED.matches_c_int(creation_status) {
            return Err(Error::NativeCodegenUnsupported);
        }
        if raw::BF_SCRIPTING_STATUS_CODEGEN_FAILED.matches_c_int(creation_status) {
            return Err(Error::NativeCodegenInitialization);
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
        compile_options: CompileOptions,
        execution_fuel: u64,
        random_seed: i32,
        debug: Option<DebugRequest<'_>>,
    ) -> Result<(Vec<Value>, Option<NativeCodegenStats>), Error> {
        let base = unsafe { raw::lua_gettop(self.pointer()) };
        let result = self.prepare_execution(random_seed).and_then(|()| {
            self.execute_loaded(
                chunk_name,
                bytecode,
                compile_options,
                execution_fuel,
                debug,
                base,
            )
        });
        unsafe { raw::lua_settop(self.pointer(), base) };
        result
    }

    fn prepare_execution(&self, random_seed: i32) -> Result<(), Error> {
        let status = unsafe { raw::bf_scripting_prepare_execution(self.pointer(), random_seed) };
        if raw::lua_Status_LUA_OK.matches_c_int(status) {
            return Ok(());
        }
        if raw::lua_Status_LUA_ERRMEM.matches_c_int(status) {
            Err(Error::OutOfMemory)
        } else {
            Err(Error::Runtime(self.runtime_error_message(-1)))
        }
    }

    fn execute_loaded(
        &self,
        chunk_name: &CString,
        bytecode: &[u8],
        compile_options: CompileOptions,
        execution_fuel: u64,
        debug: Option<DebugRequest<'_>>,
        base: i32,
    ) -> Result<(Vec<Value>, Option<NativeCodegenStats>), Error> {
        self.load_bytecode(chunk_name, bytecode)?;
        let native_codegen_stats = self.compile_native(compile_options.type_info)?;
        let (debug_options, mut debug_bridge) = match debug {
            Some(DebugRequest { options, handler }) => (
                Some(options),
                Some(DebugBridge {
                    handler,
                    panicked: false,
                }),
            ),
            None => (None, None),
        };
        if let (Some(options), Some(bridge)) = (debug_options, debug_bridge.as_mut()) {
            self.attach_debugger(options, bridge)?;
        }

        let values = self.call_loaded_chunk(base, execution_fuel, &mut debug_bridge)?;
        Ok((values, native_codegen_stats))
    }

    fn load_bytecode(&self, chunk_name: &CString, bytecode: &[u8]) -> Result<(), Error> {
        let load_status = unsafe {
            raw::luau_load(
                self.pointer(),
                chunk_name.as_ptr(),
                bytecode.as_ptr().cast(),
                bytecode.len(),
                0,
            )
        };
        if raw::lua_Status_LUA_OK.matches_c_int(load_status) {
            return Ok(());
        }

        if raw::lua_Status_LUA_ERRMEM.matches_c_int(load_status) {
            Err(Error::OutOfMemory)
        } else {
            Err(Error::Compile(self.error_message(-1)))
        }
    }

    fn attach_debugger(
        &self,
        options: &DebugOptions,
        bridge: &mut DebugBridge<'_>,
    ) -> Result<(), Error> {
        let status = unsafe {
            raw::bf_scripting_debugger_attach(
                self.pointer(),
                Some(debug_callback),
                std::ptr::from_mut(bridge).cast(),
                i32::from(options.single_step()),
            )
        };
        if !raw::BF_SCRIPTING_STATUS_OK.matches_c_int(status) {
            return Err(Error::NativeContract);
        }

        for &line in options.breakpoints() {
            if let Err(error) = self.set_breakpoint(line) {
                unsafe { raw::bf_scripting_debugger_detach(self.pointer()) };
                return Err(error);
            }
        }
        Ok(())
    }

    fn set_breakpoint(&self, line: u32) -> Result<(), Error> {
        let requested_line =
            i32::try_from(line).map_err(|_error| Error::InvalidBreakpoint { line })?;
        let mut actual_line = -1;
        let status = unsafe {
            raw::bf_scripting_debugger_set_breakpoint(
                self.pointer(),
                -1,
                requested_line,
                1,
                &raw mut actual_line,
            )
        };
        if raw::BF_SCRIPTING_STATUS_OK.matches_c_int(status) && actual_line >= 0 {
            Ok(())
        } else {
            Err(Error::InvalidBreakpoint { line })
        }
    }

    fn call_loaded_chunk(
        &self,
        base: i32,
        execution_fuel: u64,
        debug_bridge: &mut Option<DebugBridge<'_>>,
    ) -> Result<Vec<Value>, Error> {
        let debug_attached = debug_bridge.is_some();
        let begin_status =
            unsafe { raw::bf_scripting_begin_execution(self.pointer(), execution_fuel) };
        if !raw::BF_SCRIPTING_STATUS_OK.matches_c_int(begin_status) {
            if debug_attached {
                unsafe { raw::bf_scripting_debugger_detach(self.pointer()) };
            }
            return Err(Error::NativeContract);
        }

        let call_status = self.call_status(debug_attached);
        let end_status = unsafe { raw::bf_scripting_end_execution(self.pointer()) };
        if debug_attached {
            unsafe { raw::bf_scripting_debugger_detach(self.pointer()) };
        }
        if debug_bridge.as_ref().is_some_and(|bridge| bridge.panicked) {
            return Err(Error::DebugHandlerPanicked);
        }
        if raw::BF_SCRIPTING_STATUS_EXECUTION_LIMIT.matches_c_int(end_status) {
            return Err(Error::ExecutionLimit);
        }
        if !raw::BF_SCRIPTING_STATUS_OK.matches_c_int(end_status) {
            return Err(Error::NativeContract);
        }
        if !raw::lua_Status_LUA_OK.matches_c_int(call_status) {
            let error = if raw::lua_Status_LUA_ERRMEM.matches_c_int(call_status) {
                Error::OutOfMemory
            } else {
                Error::Runtime(self.runtime_error_message(-1))
            };
            return Err(error);
        }

        self.collect_results(base)
    }

    fn call_status(&self, debug_attached: bool) -> i32 {
        if !debug_attached {
            return unsafe { raw::bf_scripting_pcall(self.pointer(), 0, raw::LUA_MULTRET) };
        }

        loop {
            let status = unsafe { raw::lua_resume(self.pointer(), std::ptr::null_mut(), 0) };
            if raw::lua_Status_LUA_BREAK.matches_c_int(status) {
                continue;
            }
            if !raw::lua_Status_LUA_OK.matches_c_int(status) {
                unsafe { raw::bf_scripting_capture_debug_trace(self.pointer()) };
            }
            return status;
        }
    }

    pub(crate) fn memory_usage(&self) -> MemoryUsage {
        let usage = unsafe { raw::bf_scripting_runtime_memory_usage(&raw const self.0) };
        MemoryUsage {
            current_bytes: usage.current_bytes,
            peak_bytes: usage.peak_bytes,
            limit_bytes: usage.limit_bytes,
        }
    }

    pub(crate) fn native_codegen_memory_usage(&self) -> MemoryUsage {
        let usage =
            unsafe { raw::bf_scripting_runtime_native_codegen_memory_usage(&raw const self.0) };
        MemoryUsage {
            current_bytes: usage.current_bytes,
            peak_bytes: usage.peak_bytes,
            limit_bytes: usage.limit_bytes,
        }
    }

    fn compile_native(
        &self,
        type_info: TypeInfoLevel,
    ) -> Result<Option<NativeCodegenStats>, Error> {
        if unsafe { raw::bf_scripting_native_codegen_enabled(self.pointer()) } == 0 {
            return Ok(None);
        }

        let mut stats = raw::BFScriptingNativeCodegenStats::default();
        let status = unsafe {
            raw::bf_scripting_native_codegen_compile(
                self.pointer(),
                -1,
                type_info as i32,
                &raw mut stats,
            )
        };
        if raw::BF_SCRIPTING_STATUS_OUT_OF_MEMORY.matches_c_int(status) {
            return Err(Error::OutOfMemory);
        }
        if !raw::BF_SCRIPTING_STATUS_OK.matches_c_int(status) {
            return Err(Error::NativeCodegenCompilation);
        }
        if stats.result != 0 {
            return Ok(None);
        }

        Ok(Some(NativeCodegenStats {
            bytecode_size_bytes: stats.bytecode_size_bytes,
            native_code_size_bytes: stats.native_code_size_bytes,
            native_data_size_bytes: stats.native_data_size_bytes,
            native_metadata_size_bytes: stats.native_metadata_size_bytes,
            functions_total: stats.functions_total,
            functions_compiled: stats.functions_compiled,
            functions_bound: stats.functions_bound,
        }))
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

    fn runtime_error_message(&self, stack_index: i32) -> String {
        let message = self.error_message(stack_index);
        let trace = unsafe { raw::bf_scripting_runtime_last_debug_trace(&raw const self.0) };
        if trace.data.is_null() || trace.size == 0 {
            return message;
        }
        let trace = unsafe { slice::from_raw_parts(trace.data, trace.size) };
        format!(
            "{message}\nstack trace:\n{}",
            String::from_utf8_lossy(trace)
        )
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

unsafe extern "C" fn debug_callback(
    context: *mut c_void,
    state: *mut raw::lua_State,
    event_kind: i32,
) -> i32 {
    if context.is_null() || state.is_null() {
        return 0;
    }

    let bridge = unsafe { &mut *context.cast::<DebugBridge<'_>>() };
    let action = catch_unwind(AssertUnwindSafe(|| {
        let event = unsafe { capture_debug_event(state, event_kind) };
        bridge.handler.on_event(&event)
    }));
    match action {
        Ok(DebugAction::Continue) => 0,
        Ok(DebugAction::Step) => 1,
        Err(_panic) => {
            bridge.panicked = true;
            0
        }
    }
}

unsafe fn capture_debug_event(state: *mut raw::lua_State, event_kind: i32) -> DebugEvent {
    let _stack = StackRestore::new(state);
    let kind = if raw::BF_SCRIPTING_DEBUG_EVENT_BREAKPOINT.matches_c_int(event_kind) {
        DebugEventKind::Breakpoint
    } else {
        DebugEventKind::Step
    };
    let depth = unsafe { raw::lua_stackdepth(state) }.max(0);
    let capacity = usize::try_from(depth).unwrap_or(0);
    let mut frames = Vec::with_capacity(capacity);
    for level in 0..depth {
        let mut info = raw::lua_Debug::default();
        let found = unsafe { raw::lua_getinfo(state, level, c"sln".as_ptr(), &raw mut info) } != 0;
        if !found {
            continue;
        }
        frames.push(unsafe { capture_debug_frame(state, level, &info) });
    }
    DebugEvent { kind, frames }
}

unsafe fn capture_debug_frame(
    state: *mut raw::lua_State,
    level: i32,
    info: &raw::lua_Debug,
) -> DebugFrame {
    let depth = u32::try_from(level).unwrap_or(0);
    let source = unsafe { optional_c_string(info.source) }
        .map(|source| source.strip_prefix('=').unwrap_or(&source).to_owned());
    let function = unsafe { optional_c_string(info.name) };
    let current_line = u32::try_from(info.currentline)
        .ok()
        .filter(|line| *line > 0);
    let defined_line = u32::try_from(info.linedefined)
        .ok()
        .filter(|line| *line > 0);
    let locals = unsafe { capture_locals(state, level) };
    let upvalues = unsafe { capture_upvalues(state, level) };
    DebugFrame {
        depth,
        source,
        function,
        current_line,
        defined_line,
        locals,
        upvalues,
    }
}

unsafe fn capture_locals(state: *mut raw::lua_State, level: i32) -> Vec<DebugVariable> {
    let mut locals = Vec::new();
    for index in 1..=256 {
        let top = unsafe { raw::lua_gettop(state) };
        let name = unsafe { raw::lua_getlocal(state, level, index) };
        if name.is_null() {
            break;
        }
        let variable = DebugVariable {
            name: unsafe { CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned(),
            value: unsafe { read_debug_value(state, -1) },
        };
        unsafe { raw::lua_settop(state, top) };
        locals.push(variable);
    }
    locals
}

unsafe fn capture_upvalues(state: *mut raw::lua_State, level: i32) -> Vec<DebugVariable> {
    let base = unsafe { raw::lua_gettop(state) };
    let mut function_info = raw::lua_Debug::default();
    if unsafe { raw::lua_getinfo(state, level, c"f".as_ptr(), &raw mut function_info) } == 0 {
        return Vec::new();
    }

    let function_index = unsafe { raw::lua_gettop(state) };
    let mut upvalues = Vec::new();
    for index in 1..=256 {
        let top = unsafe { raw::lua_gettop(state) };
        let name = unsafe { raw::lua_getupvalue(state, function_index, index) };
        if name.is_null() {
            break;
        }
        let variable = DebugVariable {
            name: unsafe { CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned(),
            value: unsafe { read_debug_value(state, -1) },
        };
        unsafe { raw::lua_settop(state, top) };
        upvalues.push(variable);
    }
    unsafe { raw::lua_settop(state, base) };
    upvalues
}

unsafe fn read_debug_value(state: *mut raw::lua_State, stack_index: i32) -> DebugValue {
    let value_type = unsafe { raw::lua_type(state, stack_index) };
    match value_type {
        value if raw::lua_Type_LUA_TNIL.matches_c_int(value) => DebugValue::Nil,
        value if raw::lua_Type_LUA_TBOOLEAN.matches_c_int(value) => {
            DebugValue::Boolean(unsafe { raw::lua_toboolean(state, stack_index) } != 0)
        }
        value if raw::lua_Type_LUA_TNUMBER.matches_c_int(value) => DebugValue::Number(unsafe {
            raw::lua_tonumberx(state, stack_index, std::ptr::null_mut())
        }),
        value if raw::lua_Type_LUA_TINTEGER.matches_c_int(value) => {
            let mut is_integer = 0;
            let integer = unsafe { raw::lua_tointeger64(state, stack_index, &raw mut is_integer) };
            if is_integer == 0 {
                DebugValue::Opaque {
                    type_name: "integer".to_owned(),
                }
            } else {
                DebugValue::Integer(integer)
            }
        }
        value if raw::lua_Type_LUA_TSTRING.matches_c_int(value) => {
            let mut length = 0;
            let pointer = unsafe { raw::lua_tolstring(state, stack_index, &raw mut length) };
            if pointer.is_null() {
                DebugValue::String(Box::default())
            } else {
                DebugValue::String(
                    unsafe { slice::from_raw_parts(pointer.cast(), length) }
                        .to_vec()
                        .into_boxed_slice(),
                )
            }
        }
        value if raw::lua_Type_LUA_TVECTOR.matches_c_int(value) => {
            let pointer = unsafe { raw::lua_tovector(state, stack_index) };
            if pointer.is_null() {
                DebugValue::Opaque {
                    type_name: "vector".to_owned(),
                }
            } else {
                let components = unsafe { slice::from_raw_parts(pointer, 3) };
                DebugValue::Vector([components[0], components[1], components[2]])
            }
        }
        _ => DebugValue::Opaque {
            type_name: unsafe { debug_type_name(state, value_type) },
        },
    }
}

unsafe fn debug_type_name(state: *mut raw::lua_State, value_type: i32) -> String {
    let pointer = unsafe { raw::lua_typename(state, value_type) };
    if pointer.is_null() {
        "unknown".to_owned()
    } else {
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }
}

unsafe fn optional_c_string(pointer: *const std::ffi::c_char) -> Option<String> {
    (!pointer.is_null()).then(|| {
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    })
}

struct StackRestore {
    state: *mut raw::lua_State,
    top: i32,
}

impl StackRestore {
    unsafe fn new(state: *mut raw::lua_State) -> Self {
        Self {
            state,
            top: unsafe { raw::lua_gettop(state) },
        }
    }
}

impl Drop for StackRestore {
    fn drop(&mut self) {
        unsafe { raw::lua_settop(self.state, self.top) };
    }
}
