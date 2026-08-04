#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw Luau calls and native pointer materialization are isolated in this private module"
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
#[allow(
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks,
    reason = "bindgen output is generated from the pinned Luau headers"
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
    // SAFETY: this wrapper query takes no pointers and returns a value record.
    let version = unsafe { raw::bf_scripting_luau_version() };
    (version.major, version.minor, version.patch)
}

pub(crate) fn native_codegen_supported() -> bool {
    // SAFETY: this capability query takes no pointers and returns a scalar flag.
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
    // SAFETY: source and options remain readable for the call and `bytecode` is
    // a valid, uniquely writable output record.
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
        // SAFETY: the compiler returned this non-null allocation and ownership
        // is released exactly once on the invalid empty-output path.
        unsafe { raw::bf_scripting_bytecode_free(pointer.as_ptr().cast()) };
        return Err(Error::NativeContract);
    }
    // SAFETY: a successful compile returns `size` initialized bytes owned by
    // `pointer`; they are copied before the allocation is freed.
    let bytes = unsafe { slice::from_raw_parts(pointer.as_ptr(), bytecode.size) }.to_vec();
    // SAFETY: the compiler returned this allocation and the copied bytes no
    // longer borrow it, so ownership is released exactly once here.
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
        // SAFETY: `runtime` is a valid, uniquely writable output record and both
        // memory limits are validated by `RuntimeConfig`.
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
        // SAFETY: the runtime owns a live VM pointer and the scalar seed/library
        // mask are passed by value during one-time initialization.
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
        // SAFETY: the runtime owns a live VM and this exclusive execution call
        // serializes access to its Lua stack.
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
        // SAFETY: `base` was captured from this same live VM before execution,
        // restoring the stack after all result copies are complete.
        unsafe { raw::lua_settop(self.pointer(), base) };
        result
    }

    fn prepare_execution(&self, random_seed: i32) -> Result<(), Error> {
        // SAFETY: the VM is live and execution is serialized by the safe runtime.
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
        // SAFETY: the VM is live; `chunk_name` is NUL-terminated; authenticated
        // bytecode remains readable for its explicit length during the load.
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
        // SAFETY: the VM is live, `bridge` remains at a stable address until
        // debugger detach, and `debug_callback` has the required C ABI.
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
                // SAFETY: this VM has the debugger attached by the successful
                // call above and no callback is active on this thread.
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
        // SAFETY: the VM is live with an attached debugger and `actual_line` is
        // a valid, uniquely writable scalar output.
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
        // SAFETY: the VM is live, execution is serialized, and the fuel counter
        // is installed before entering Luau.
        let begin_status =
            unsafe { raw::bf_scripting_begin_execution(self.pointer(), execution_fuel) };
        if !raw::BF_SCRIPTING_STATUS_OK.matches_c_int(begin_status) {
            if debug_attached {
                // SAFETY: `debug_attached` records a successful attach on this VM.
                unsafe { raw::bf_scripting_debugger_detach(self.pointer()) };
            }
            return Err(Error::NativeContract);
        }

        let call_status = self.call_status(debug_attached);
        // SAFETY: begin-execution succeeded on this VM and the protected call
        // has returned, so the matching execution scope can be ended.
        let end_status = unsafe { raw::bf_scripting_end_execution(self.pointer()) };
        if debug_attached {
            // SAFETY: `debug_attached` records a successful attach on this VM
            // and no callback is active after the protected call returned.
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
            // SAFETY: the live VM has a loaded function at the stack top and the
            // execution scope is active; results remain on the VM stack.
            return unsafe { raw::bf_scripting_pcall(self.pointer(), 0, raw::LUA_MULTRET) };
        }

        loop {
            // SAFETY: the debugger is attached to this live VM and execution is
            // resumed only after each synchronous break callback returns.
            let status = unsafe { raw::lua_resume(self.pointer(), std::ptr::null_mut(), 0) };
            if raw::lua_Status_LUA_BREAK.matches_c_int(status) {
                continue;
            }
            if !raw::lua_Status_LUA_OK.matches_c_int(status) {
                // SAFETY: the VM is live and suspended/failed; the wrapper copies
                // its debug trace into runtime-owned storage.
                unsafe { raw::bf_scripting_capture_debug_trace(self.pointer()) };
            }
            return status;
        }
    }

    pub(crate) fn memory_usage(&self) -> MemoryUsage {
        // SAFETY: `self.0` is a live runtime record and the query only reads its
        // allocator counters.
        let usage = unsafe { raw::bf_scripting_runtime_memory_usage(&raw const self.0) };
        MemoryUsage {
            current_bytes: usage.current_bytes,
            peak_bytes: usage.peak_bytes,
            limit_bytes: usage.limit_bytes,
        }
    }

    pub(crate) fn native_codegen_memory_usage(&self) -> MemoryUsage {
        // SAFETY: `self.0` is a live runtime record and the query only reads its
        // code allocator counters.
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
        // SAFETY: the VM is live and this capability query does not mutate stack state.
        if unsafe { raw::bf_scripting_native_codegen_enabled(self.pointer()) } == 0 {
            return Ok(None);
        }

        let mut stats = raw::BFScriptingNativeCodegenStats::default();
        // SAFETY: the VM is live with the loaded function at stack index -1 and
        // `stats` is a valid, uniquely writable output record.
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
        // SAFETY: the VM is live and execution has returned while preserving its
        // result values on the serialized stack.
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
        // SAFETY: `stack_index` is within the live result range established by
        // `collect_results` on this serialized VM stack.
        let value_type = unsafe { raw::lua_type(self.pointer(), stack_index) };
        match value_type {
            value if raw::lua_Type_LUA_TNIL.matches_c_int(value) => Ok(Value::Nil),
            value if raw::lua_Type_LUA_TBOOLEAN.matches_c_int(value) => Ok(Value::Boolean(
                // SAFETY: the type check above established a boolean at `stack_index`.
                unsafe { raw::lua_toboolean(self.pointer(), stack_index) } != 0,
            )),
            value if raw::lua_Type_LUA_TNUMBER.matches_c_int(value) => {
                // SAFETY: the type check above established a number at
                // `stack_index`; no conversion-success output is required.
                Ok(Value::Number(unsafe {
                    raw::lua_tonumberx(self.pointer(), stack_index, std::ptr::null_mut())
                }))
            }
            value if raw::lua_Type_LUA_TINTEGER.matches_c_int(value) => {
                let mut is_integer = 0;
                // SAFETY: the type check established an integer and `is_integer`
                // is a uniquely writable scalar output.
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
                    // SAFETY: the type check established a vector at `stack_index`.
                    unsafe { raw::lua_tovector(self.pointer(), stack_index) }.cast_mut(),
                )
                .ok_or(Error::NativeContract)?;
                // SAFETY: Luau vectors contain exactly three contiguous float
                // components and the VM stack keeps them alive during this copy.
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
            // SAFETY: `stack_index` was type-checked as a string and `length` is
            // a uniquely writable output for the live VM value.
            unsafe { raw::lua_tolstring(self.pointer(), stack_index, &raw mut length) }.cast_mut(),
        )
        .ok_or(Error::NativeContract)?;
        Ok(
            // SAFETY: Luau returned `length` bytes owned by the live stack value;
            // they are copied before stack restoration.
            unsafe { slice::from_raw_parts(pointer.as_ptr().cast(), length) }
                .to_vec()
                .into_boxed_slice(),
        )
    }

    fn type_name(&self, value_type: i32) -> String {
        // SAFETY: the VM is live and Luau accepts every returned type tag.
        let pointer = unsafe { raw::lua_typename(self.pointer(), value_type) };
        if pointer.is_null() {
            return "unknown".to_owned();
        }
        // SAFETY: the non-null pointer above addresses Luau's process-lifetime
        // NUL-terminated type name.
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }

    fn error_message(&self, stack_index: i32) -> String {
        let mut length = 0;
        // SAFETY: `stack_index` refers to a live VM error value and `length` is
        // a uniquely writable output.
        let pointer = unsafe { raw::lua_tolstring(self.pointer(), stack_index, &raw mut length) };
        if pointer.is_null() {
            return "unknown native error".to_owned();
        }
        // SAFETY: Luau returned `length` bytes owned by the live stack value;
        // they are copied into the returned `String`.
        let bytes = unsafe { slice::from_raw_parts(pointer.cast(), length) };
        String::from_utf8_lossy(bytes).into_owned()
    }

    fn runtime_error_message(&self, stack_index: i32) -> String {
        let message = self.error_message(stack_index);
        // SAFETY: `self.0` is a live runtime record whose trace storage remains
        // valid until the next execution or runtime destruction.
        let trace = unsafe { raw::bf_scripting_runtime_last_debug_trace(&raw const self.0) };
        if trace.data.is_null() || trace.size == 0 {
            return message;
        }
        // SAFETY: the non-null runtime-owned trace contains `size` readable
        // bytes and is copied into the formatted result immediately.
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
        // SAFETY: `self.0` uniquely owns the runtime and its VM; the wrapper
        // accepts partially initialized records and releases them exactly once.
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

    // SAFETY: debugger attach supplied this pointer to a live `DebugBridge` and
    // the synchronous callback cannot outlive the execution call.
    let bridge = unsafe { &mut *context.cast::<DebugBridge<'_>>() };
    let action = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Luau supplies its live callback VM and a documented event tag.
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
    // SAFETY: `state` is the live VM supplied to the debugger callback.
    let depth = unsafe { raw::lua_stackdepth(state) }.max(0);
    let capacity = usize::try_from(depth).unwrap_or(0);
    let mut frames = Vec::with_capacity(capacity);
    for level in 0..depth {
        let mut info = raw::lua_Debug::default();
        // SAFETY: `level` is within the depth returned by this VM, the query
        // selector is NUL-terminated, and `info` is uniquely writable.
        let found = unsafe { raw::lua_getinfo(state, level, c"sln".as_ptr(), &raw mut info) } != 0;
        if !found {
            continue;
        }
        // SAFETY: `info` was initialized successfully for this live VM frame.
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
    // SAFETY: a successful `lua_getinfo` supplied either null or a valid source string.
    let source = unsafe { optional_c_string(info.source) }
        .map(|source| source.strip_prefix('=').unwrap_or(&source).to_owned());
    // SAFETY: a successful `lua_getinfo` supplied either null or a valid function name.
    let function = unsafe { optional_c_string(info.name) };
    let current_line = u32::try_from(info.currentline)
        .ok()
        .filter(|line| *line > 0);
    let defined_line = u32::try_from(info.linedefined)
        .ok()
        .filter(|line| *line > 0);
    // SAFETY: `state` and `level` identify the live frame described by `info`.
    let locals = unsafe { capture_locals(state, level) };
    // SAFETY: `state` and `level` identify the live frame described by `info`.
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
        // SAFETY: `state` is a live callback VM and stack access is serialized.
        let top = unsafe { raw::lua_gettop(state) };
        // SAFETY: `level` identifies a live frame; Luau returns null when
        // `index` exceeds its local-variable range and otherwise pushes a value.
        let name = unsafe { raw::lua_getlocal(state, level, index) };
        if name.is_null() {
            break;
        }
        let variable = DebugVariable {
            // SAFETY: a non-null local name is a NUL-terminated string owned by Luau.
            name: unsafe { CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned(),
            // SAFETY: `lua_getlocal` pushed the variable value at stack index -1.
            value: unsafe { read_debug_value(state, -1) },
        };
        // SAFETY: `top` was captured from this VM immediately before the local
        // query and restores the temporary pushed value.
        unsafe { raw::lua_settop(state, top) };
        locals.push(variable);
    }
    locals
}

unsafe fn capture_upvalues(state: *mut raw::lua_State, level: i32) -> Vec<DebugVariable> {
    // SAFETY: `state` is a live callback VM and stack access is serialized.
    let base = unsafe { raw::lua_gettop(state) };
    let mut function_info = raw::lua_Debug::default();
    // SAFETY: `level` identifies a live frame, the selector is NUL-terminated,
    // and `function_info` is uniquely writable; success pushes the function.
    if unsafe { raw::lua_getinfo(state, level, c"f".as_ptr(), &raw mut function_info) } == 0 {
        return Vec::new();
    }

    // SAFETY: successful `lua_getinfo(..., "f")` pushed the frame function.
    let function_index = unsafe { raw::lua_gettop(state) };
    let mut upvalues = Vec::new();
    for index in 1..=256 {
        // SAFETY: `state` is live and stack access is serialized.
        let top = unsafe { raw::lua_gettop(state) };
        // SAFETY: `function_index` refers to the pushed frame function; Luau
        // returns null at the end and otherwise pushes one upvalue.
        let name = unsafe { raw::lua_getupvalue(state, function_index, index) };
        if name.is_null() {
            break;
        }
        let variable = DebugVariable {
            // SAFETY: a non-null upvalue name is NUL-terminated and owned by Luau.
            name: unsafe { CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned(),
            // SAFETY: `lua_getupvalue` pushed the value at stack index -1.
            value: unsafe { read_debug_value(state, -1) },
        };
        // SAFETY: `top` restores the stack after the temporary upvalue push.
        unsafe { raw::lua_settop(state, top) };
        upvalues.push(variable);
    }
    // SAFETY: `base` was captured before the temporary function push and
    // restores the callback VM stack before returning.
    unsafe { raw::lua_settop(state, base) };
    upvalues
}

unsafe fn read_debug_value(state: *mut raw::lua_State, stack_index: i32) -> DebugValue {
    // SAFETY: the caller guarantees `stack_index` identifies a live VM value.
    let value_type = unsafe { raw::lua_type(state, stack_index) };
    match value_type {
        value if raw::lua_Type_LUA_TNIL.matches_c_int(value) => DebugValue::Nil,
        value if raw::lua_Type_LUA_TBOOLEAN.matches_c_int(value) => {
            // SAFETY: the type check established a boolean at `stack_index`.
            DebugValue::Boolean(unsafe { raw::lua_toboolean(state, stack_index) } != 0)
        }
        value if raw::lua_Type_LUA_TNUMBER.matches_c_int(value) => {
            // SAFETY: the type check established a number at `stack_index`.
            DebugValue::Number(unsafe {
                raw::lua_tonumberx(state, stack_index, std::ptr::null_mut())
            })
        }
        value if raw::lua_Type_LUA_TINTEGER.matches_c_int(value) => {
            let mut is_integer = 0;
            // SAFETY: the type check established an integer and `is_integer` is
            // a uniquely writable conversion output.
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
            // SAFETY: the type check established a string and `length` is a
            // uniquely writable output.
            let pointer = unsafe { raw::lua_tolstring(state, stack_index, &raw mut length) };
            if pointer.is_null() {
                DebugValue::String(Box::default())
            } else {
                DebugValue::String(
                    // SAFETY: Luau returned `length` bytes owned by this live
                    // stack value; they are copied immediately.
                    unsafe { slice::from_raw_parts(pointer.cast(), length) }
                        .to_vec()
                        .into_boxed_slice(),
                )
            }
        }
        value if raw::lua_Type_LUA_TVECTOR.matches_c_int(value) => {
            // SAFETY: the type check established a vector at `stack_index`.
            let pointer = unsafe { raw::lua_tovector(state, stack_index) };
            if pointer.is_null() {
                DebugValue::Opaque {
                    type_name: "vector".to_owned(),
                }
            } else {
                // SAFETY: Luau vectors contain exactly three contiguous floats
                // owned by the live stack value.
                let components = unsafe { slice::from_raw_parts(pointer, 3) };
                DebugValue::Vector([components[0], components[1], components[2]])
            }
        }
        _ => DebugValue::Opaque {
            // SAFETY: `value_type` was returned by this live VM.
            type_name: unsafe { debug_type_name(state, value_type) },
        },
    }
}

unsafe fn debug_type_name(state: *mut raw::lua_State, value_type: i32) -> String {
    // SAFETY: `state` is live and Luau accepts every type tag it returned.
    let pointer = unsafe { raw::lua_typename(state, value_type) };
    if pointer.is_null() {
        "unknown".to_owned()
    } else {
        // SAFETY: a non-null type-name pointer is a process-lifetime
        // NUL-terminated Luau string.
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }
}

unsafe fn optional_c_string(pointer: *const std::ffi::c_char) -> Option<String> {
    (!pointer.is_null()).then(|| {
        // SAFETY: callers only pass optional Luau debug strings, which are
        // NUL-terminated whenever non-null.
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
            // SAFETY: the caller guarantees `state` is a live callback VM.
            top: unsafe { raw::lua_gettop(state) },
        }
    }
}

impl Drop for StackRestore {
    fn drop(&mut self) {
        // SAFETY: this guard cannot outlive the callback's live VM and `top` was
        // captured from the same serialized stack.
        unsafe { raw::lua_settop(self.state, self.top) };
    }
}
