#include "wrapper.h"

#include "Luau/CodeGen.h"
#include "Luau/CodeGenOptions.h"
#include "luacode.h"

#include <algorithm>
#include <cstdlib>
#include <new>
#include <string>

static_assert(LUA_USE_LONGJMP == 1, "Luau VM and wrapper must use longjmp errors");
static_assert(LUA_IDSIZE == 256, "Luau debug record layout must be pinned");
static_assert(LUA_VECTOR_SIZE == 3, "Luau vector layout must be pinned");
static_assert(LUA_VECTOR_DOUBLE == 0, "Luau vector scalar layout must be pinned");

namespace {

constexpr size_t NATIVE_CODEGEN_BLOCK_SIZE = 4 * 1024 * 1024;
constexpr size_t STRING_OPERATION_LIMIT_BYTES = 64 * 1024;
constexpr const char *BASE_GLOBALS_REGISTRY_KEY = "blackflower.base_globals";

struct RuntimeContext {
    size_t current_bytes;
    size_t peak_bytes;
    size_t limit_bytes;
    size_t native_codegen_current_bytes;
    size_t native_codegen_peak_bytes;
    size_t native_codegen_limit_bytes;
    uint64_t remaining_fuel;
    bool execution_active;
    bool execution_limit_reached;
    BFScriptingDebugCallback debug_callback;
    void *debug_callback_context;
    bool native_execution_suspended;
    bool resuming_breakpoint;
    bool resuming_step;
    std::string last_debug_trace;
};

struct InitializeContext {
    int32_t random_seed;
    uint32_t libraries;
};

struct PrepareExecutionContext {
    int32_t random_seed;
};

struct LibraryRegistration {
    uint32_t mask;
    const char *name;
    lua_CFunction open;
};

void open_library(lua_State *state, const char *name, lua_CFunction function) {
    lua_pushcfunction(state, function, nullptr);
    lua_pushstring(state, name);
    lua_call(state, 1, 0);
}

int call_bounded_string_builtin(lua_State *state, bool is_repetition) {
    const int argument_count = lua_gettop(state);
    for (int index = 1; index <= argument_count; ++index) {
        if (lua_type(state, index) == LUA_TSTRING
            && static_cast<size_t>(lua_objlen(state, index))
                > STRING_OPERATION_LIMIT_BYTES) {
            luaL_error(state, "string argument exceeds sandbox limit");
        }
    }

    if (is_repetition) {
        size_t input_size = 0;
        luaL_checklstring(state, 1, &input_size);
        const int repetitions = luaL_checkinteger(state, 2);
        if (repetitions > 0
            && input_size
                > STRING_OPERATION_LIMIT_BYTES
                    / static_cast<size_t>(repetitions)) {
            luaL_error(state, "string result exceeds sandbox limit");
        }
    }

    lua_pushvalue(state, lua_upvalueindex(1));
    lua_insert(state, 1);
    lua_call(state, argument_count, LUA_MULTRET);

    const int result_count = lua_gettop(state);
    for (int index = 1; index <= result_count; ++index) {
        if (lua_type(state, index) == LUA_TSTRING
            && static_cast<size_t>(lua_objlen(state, index))
                > STRING_OPERATION_LIMIT_BYTES) {
            luaL_error(state, "string result exceeds sandbox limit");
        }
    }
    return result_count;
}

int bounded_string_builtin(lua_State *state) {
    return call_bounded_string_builtin(state, false);
}

int bounded_string_repetition(lua_State *state) {
    return call_bounded_string_builtin(state, true);
}

void wrap_string_builtin(
    lua_State *state,
    const char *name,
    lua_CFunction wrapper) {
    lua_getfield(state, -1, name);
    if (!lua_isfunction(state, -1)) {
        luaL_error(state, "missing Luau string builtin");
    }
    lua_pushcclosure(state, wrapper, name, 1);
    lua_setfield(state, -2, name);
}

void remove_string_builtin(lua_State *state, const char *name) {
    lua_pushnil(state);
    lua_setfield(state, -2, name);
}

void harden_string_library(lua_State *state) {
    lua_getglobal(state, LUA_STRLIBNAME);
    if (!lua_istable(state, -1)) {
        luaL_error(state, "missing Luau string library");
    }

    const char *bounded_builtins[] = {
        "byte",
        "char",
        "len",
        "lower",
        "reverse",
        "sub",
        "upper",
        "split",
    };
    for (const char *name : bounded_builtins) {
        wrap_string_builtin(state, name, bounded_string_builtin);
    }
    wrap_string_builtin(state, "rep", bounded_string_repetition);

    const char *disabled_builtins[] = {
        "find",
        "format",
        "gmatch",
        "gsub",
        "match",
        "pack",
        "packsize",
        "unpack",
    };
    for (const char *name : disabled_builtins) {
        remove_string_builtin(state, name);
    }
    lua_pop(state, 1);
}

RuntimeContext *runtime_context(lua_State *state) {
    if (state == nullptr) {
        return nullptr;
    }
    return static_cast<RuntimeContext *>(lua_callbacks(state)->userdata);
}

void *limited_alloc(
    void *userdata,
    void *pointer,
    size_t old_size,
    size_t new_size) {
    auto *context = static_cast<RuntimeContext *>(userdata);
    if (context == nullptr || old_size > context->current_bytes) {
        return nullptr;
    }

    const size_t retained_bytes = context->current_bytes - old_size;
    if (new_size == 0) {
        std::free(pointer);
        context->current_bytes = retained_bytes;
        return nullptr;
    }
    if (new_size > context->limit_bytes - retained_bytes) {
        return nullptr;
    }

    void *result = std::realloc(pointer, new_size);
    if (result == nullptr) {
        return nullptr;
    }

    context->current_bytes = retained_bytes + new_size;
    context->peak_bytes =
        std::max(context->peak_bytes, context->current_bytes);
    return result;
}

void native_codegen_allocation(
    void *userdata,
    void *,
    size_t old_size,
    void *,
    size_t new_size) {
    auto *context = static_cast<RuntimeContext *>(userdata);
    if (context == nullptr || old_size > context->native_codegen_current_bytes) {
        return;
    }

    context->native_codegen_current_bytes =
        context->native_codegen_current_bytes - old_size + new_size;
    context->native_codegen_peak_bytes = std::max(
        context->native_codegen_peak_bytes,
        context->native_codegen_current_bytes);
}

std::string build_debug_trace(lua_State *state) {
    std::string trace;
    const int depth = lua_stackdepth(state);
    for (int level = 0; level < depth; ++level) {
        lua_Debug frame {};
        if (lua_getinfo(state, level, "sln", &frame) == 0) {
            continue;
        }

        trace += frame.short_src[0] != '\0' ? frame.short_src : "<unknown>";
        if (frame.currentline > 0) {
            trace += ":";
            trace += std::to_string(frame.currentline);
        }
        if (frame.name != nullptr) {
            trace += " function ";
            trace += frame.name;
        }
        trace += "\n";
    }
    return trace;
}

void capture_protected_error(lua_State *state) {
    RuntimeContext *context = runtime_context(state);
    if (context == nullptr) {
        return;
    }

    try {
        context->last_debug_trace = build_debug_trace(state);
    } catch (...) {
        context->last_debug_trace.clear();
    }
}

void dispatch_debug_event(lua_State *state, int32_t event_kind) {
    RuntimeContext *context = runtime_context(state);
    if (context == nullptr || context->debug_callback == nullptr) {
        return;
    }
    if (event_kind == BF_SCRIPTING_DEBUG_EVENT_BREAKPOINT
        && context->resuming_breakpoint) {
        context->resuming_breakpoint = false;
        return;
    }
    if (event_kind == BF_SCRIPTING_DEBUG_EVENT_STEP
        && context->resuming_step) {
        context->resuming_step = false;
        return;
    }

    const int32_t action = context->debug_callback(
        context->debug_callback_context,
        state,
        event_kind);
    if (action == BF_SCRIPTING_DEBUG_ACTION_STEP) {
        lua_singlestep(state, 1);
        context->resuming_breakpoint =
            event_kind == BF_SCRIPTING_DEBUG_EVENT_BREAKPOINT;
        context->resuming_step =
            event_kind == BF_SCRIPTING_DEBUG_EVENT_STEP;
        lua_break(state);
    } else {
        lua_singlestep(state, 0);
    }
}

void debug_break(lua_State *state, lua_Debug *) {
    dispatch_debug_event(state, BF_SCRIPTING_DEBUG_EVENT_BREAKPOINT);
}

void debug_step(lua_State *state, lua_Debug *) {
    dispatch_debug_event(state, BF_SCRIPTING_DEBUG_EVENT_STEP);
}

void execution_interrupt(lua_State *state, int gc) {
    if (gc >= 0) {
        return;
    }

    RuntimeContext *context = runtime_context(state);
    if (context == nullptr || !context->execution_active) {
        return;
    }
    if (context->remaining_fuel > 0) {
        --context->remaining_fuel;
        return;
    }

    context->execution_limit_reached = true;
    luaL_error(state, "execution fuel exhausted");
}

void seed_random(lua_State *state, int32_t random_seed) {
    lua_getglobal(state, LUA_MATHLIBNAME);
    if (!lua_istable(state, -1)) {
        lua_pop(state, 1);
        return;
    }

    lua_getfield(state, -1, "randomseed");
    if (!lua_isfunction(state, -1)) {
        lua_pop(state, 2);
        return;
    }
    lua_pushinteger(state, random_seed);
    lua_call(state, 1, 0);
    lua_pop(state, 1);
}

void reset_execution_environment(lua_State *state, int32_t random_seed) {
    lua_newtable(state);

    lua_newtable(state);
    lua_getfield(state, LUA_REGISTRYINDEX, BASE_GLOBALS_REGISTRY_KEY);
    if (!lua_istable(state, -1)) {
        luaL_error(state, "missing immutable base globals");
    }
    lua_setfield(state, -2, "__index");
    lua_setreadonly(state, -1, true);
    lua_setmetatable(state, -2);

    lua_replace(state, LUA_GLOBALSINDEX);
    lua_setsafeenv(state, LUA_GLOBALSINDEX, true);
    lua_pushvalue(state, LUA_GLOBALSINDEX);
    lua_setfield(state, LUA_GLOBALSINDEX, "_G");
    seed_random(state, random_seed);
}

int prepare_execution(lua_State *state) {
    const auto *context =
        static_cast<const PrepareExecutionContext *>(lua_touserdata(state, 1));
    reset_execution_environment(state, context->random_seed);
    return 0;
}

int initialize_runtime(lua_State *state) {
    const auto *context =
        static_cast<const InitializeContext *>(lua_touserdata(state, 1));

    const LibraryRegistration registrations[] = {
        {BF_SCRIPTING_LIBRARY_BASE, "", luaopen_base},
        {BF_SCRIPTING_LIBRARY_COROUTINE, LUA_COLIBNAME, luaopen_coroutine},
        {BF_SCRIPTING_LIBRARY_TABLE, LUA_TABLIBNAME, luaopen_table},
        {BF_SCRIPTING_LIBRARY_STRING, LUA_STRLIBNAME, luaopen_string},
        {BF_SCRIPTING_LIBRARY_MATH, LUA_MATHLIBNAME, luaopen_math},
        {BF_SCRIPTING_LIBRARY_UTF8, LUA_UTF8LIBNAME, luaopen_utf8},
        {BF_SCRIPTING_LIBRARY_BIT32, LUA_BITLIBNAME, luaopen_bit32},
        {BF_SCRIPTING_LIBRARY_BUFFER, LUA_BUFFERLIBNAME, luaopen_buffer},
        {BF_SCRIPTING_LIBRARY_VECTOR, LUA_VECLIBNAME, luaopen_vector},
        {BF_SCRIPTING_LIBRARY_INTEGER, LUA_INTLIBNAME, luaopen_integer},
    };
    for (const LibraryRegistration &registration : registrations) {
        if ((context->libraries & registration.mask) != 0) {
            open_library(state, registration.name, registration.open);
            if (registration.mask == BF_SCRIPTING_LIBRARY_STRING) {
                harden_string_library(state);
            }
        }
    }

    luaL_sandbox(state);
    lua_pushvalue(state, LUA_GLOBALSINDEX);
    lua_setfield(state, LUA_REGISTRYINDEX, BASE_GLOBALS_REGISTRY_KEY);
    reset_execution_environment(state, context->random_seed);
    return 0;
}

bool valid_options(const BFScriptingCompileOptions &options) {
    return options.optimization_level >= 0 && options.optimization_level <= 2
        && options.debug_level >= 0 && options.debug_level <= 2
        && options.type_info_level >= 0 && options.type_info_level <= 1
        && options.coverage_level >= 0 && options.coverage_level <= 2;
}

} // namespace

extern "C" BFScriptingVersion bf_scripting_luau_version() {
    return BFScriptingVersion {
        BF_LUAU_VERSION_MAJOR,
        BF_LUAU_VERSION_MINOR,
        BF_LUAU_VERSION_PATCH,
    };
}

extern "C" int32_t bf_scripting_runtime_new(
    size_t memory_limit_bytes,
    size_t native_codegen_limit_bytes,
    BFScriptingRuntime *out_runtime) {
    if (out_runtime == nullptr) {
        return BF_SCRIPTING_STATUS_NULL_POINTER;
    }
    out_runtime->state = nullptr;
    out_runtime->context = nullptr;
    if (memory_limit_bytes == 0) {
        return BF_SCRIPTING_STATUS_INVALID_ARGUMENT;
    }

    auto *context = new (std::nothrow) RuntimeContext {
        0,
        0,
        memory_limit_bytes,
        0,
        0,
        native_codegen_limit_bytes,
        0,
        false,
        false,
        nullptr,
        nullptr,
        false,
        false,
        false,
        {},
    };
    if (context == nullptr) {
        return BF_SCRIPTING_STATUS_OUT_OF_MEMORY;
    }

    lua_State *state = lua_newstate(limited_alloc, context);
    if (state == nullptr) {
        delete context;
        return BF_SCRIPTING_STATUS_OUT_OF_MEMORY;
    }

    lua_callbacks(state)->userdata = context;
    lua_callbacks(state)->debugprotectederror = capture_protected_error;
    if (native_codegen_limit_bytes != 0) {
        if (!Luau::CodeGen::isSupported()) {
            lua_close(state);
            delete context;
            return BF_SCRIPTING_STATUS_CODEGEN_UNSUPPORTED;
        }

        try {
            const size_t block_size =
                std::min(NATIVE_CODEGEN_BLOCK_SIZE, native_codegen_limit_bytes);
            Luau::CodeGen::create(
                state,
                block_size,
                native_codegen_limit_bytes,
                native_codegen_allocation,
                context);
        } catch (const std::bad_alloc &) {
            lua_close(state);
            delete context;
            return BF_SCRIPTING_STATUS_OUT_OF_MEMORY;
        } catch (...) {
            lua_close(state);
            delete context;
            return BF_SCRIPTING_STATUS_CODEGEN_FAILED;
        }
        if (!Luau::CodeGen::isNativeExecutionEnabled(state)) {
            lua_close(state);
            delete context;
            return BF_SCRIPTING_STATUS_CODEGEN_FAILED;
        }
    }

    out_runtime->state = state;
    out_runtime->context = context;
    return BF_SCRIPTING_STATUS_OK;
}

extern "C" void bf_scripting_runtime_free(BFScriptingRuntime *runtime) {
    if (runtime == nullptr) {
        return;
    }

    auto *context = static_cast<RuntimeContext *>(runtime->context);
    if (runtime->state != nullptr) {
        lua_callbacks(runtime->state)->interrupt = nullptr;
        lua_callbacks(runtime->state)->debugbreak = nullptr;
        lua_callbacks(runtime->state)->debugstep = nullptr;
        lua_callbacks(runtime->state)->debugprotectederror = nullptr;
        lua_close(runtime->state);
    }
    delete context;
    runtime->state = nullptr;
    runtime->context = nullptr;
}

extern "C" BFScriptingMemoryUsage bf_scripting_runtime_memory_usage(
    const BFScriptingRuntime *runtime) {
    if (runtime == nullptr || runtime->context == nullptr) {
        return BFScriptingMemoryUsage {0, 0, 0};
    }

    const auto *context =
        static_cast<const RuntimeContext *>(runtime->context);
    return BFScriptingMemoryUsage {
        context->current_bytes,
        context->peak_bytes,
        context->limit_bytes,
    };
}

extern "C" BFScriptingMemoryUsage
bf_scripting_runtime_native_codegen_memory_usage(
    const BFScriptingRuntime *runtime) {
    if (runtime == nullptr || runtime->context == nullptr) {
        return BFScriptingMemoryUsage {0, 0, 0};
    }

    const auto *context =
        static_cast<const RuntimeContext *>(runtime->context);
    return BFScriptingMemoryUsage {
        context->native_codegen_current_bytes,
        context->native_codegen_peak_bytes,
        context->native_codegen_limit_bytes,
    };
}

extern "C" BFScriptingBytesView bf_scripting_runtime_last_debug_trace(
    const BFScriptingRuntime *runtime) {
    if (runtime == nullptr || runtime->context == nullptr) {
        return BFScriptingBytesView {nullptr, 0};
    }

    const auto *context =
        static_cast<const RuntimeContext *>(runtime->context);
    return BFScriptingBytesView {
        reinterpret_cast<const uint8_t *>(context->last_debug_trace.data()),
        context->last_debug_trace.size(),
    };
}

extern "C" int32_t bf_scripting_initialize(
    lua_State *state,
    int32_t random_seed,
    uint32_t libraries) {
    if (state == nullptr) {
        return LUA_ERRRUN;
    }
    if ((libraries & ~BF_SCRIPTING_LIBRARY_ALL) != 0) {
        return BF_SCRIPTING_STATUS_INVALID_ARGUMENT;
    }

    InitializeContext context {random_seed, libraries};
    return lua_cpcall(state, initialize_runtime, &context);
}

extern "C" int32_t bf_scripting_begin_execution(
    lua_State *state,
    uint64_t fuel) {
    RuntimeContext *context = runtime_context(state);
    if (context == nullptr) {
        return BF_SCRIPTING_STATUS_NULL_POINTER;
    }
    if (fuel == 0 || context->execution_active) {
        return BF_SCRIPTING_STATUS_INVALID_ARGUMENT;
    }

    context->remaining_fuel = fuel;
    context->execution_active = true;
    context->execution_limit_reached = false;
    context->last_debug_trace.clear();
    lua_callbacks(state)->interrupt = execution_interrupt;
    return BF_SCRIPTING_STATUS_OK;
}

extern "C" int32_t bf_scripting_prepare_execution(
    lua_State *state,
    int32_t random_seed) {
    if (state == nullptr) {
        return BF_SCRIPTING_STATUS_NULL_POINTER;
    }
    RuntimeContext *runtime = runtime_context(state);
    if (runtime == nullptr) {
        return BF_SCRIPTING_STATUS_NULL_POINTER;
    }
    if (runtime->execution_active) {
        return BF_SCRIPTING_STATUS_INVALID_ARGUMENT;
    }

    PrepareExecutionContext context {random_seed};
    return lua_cpcall(state, prepare_execution, &context);
}

extern "C" int32_t bf_scripting_end_execution(lua_State *state) {
    RuntimeContext *context = runtime_context(state);
    if (context == nullptr) {
        return BF_SCRIPTING_STATUS_NULL_POINTER;
    }
    if (!context->execution_active) {
        return BF_SCRIPTING_STATUS_INVALID_ARGUMENT;
    }

    lua_callbacks(state)->interrupt = nullptr;
    context->remaining_fuel = 0;
    context->execution_active = false;
    return context->execution_limit_reached
        ? BF_SCRIPTING_STATUS_EXECUTION_LIMIT
        : BF_SCRIPTING_STATUS_OK;
}

extern "C" int32_t bf_scripting_pcall(
    lua_State *state,
    int32_t argument_count,
    int32_t result_count) {
    if (state == nullptr) {
        return BF_SCRIPTING_STATUS_NULL_POINTER;
    }
    if (argument_count < 0
        || lua_gettop(state) < argument_count + 1) {
        return BF_SCRIPTING_STATUS_INVALID_ARGUMENT;
    }

    const int function_index = lua_gettop(state) - argument_count;
    lua_pushcfunction(
        state,
        [](lua_State *error_state) -> int {
            capture_protected_error(error_state);
            return 1;
        },
        "blackflower_error_handler");
    lua_insert(state, function_index);
    const int status =
        lua_pcall(state, argument_count, result_count, function_index);
    lua_remove(state, function_index);
    return status;
}

extern "C" void bf_scripting_capture_debug_trace(lua_State *state) {
    capture_protected_error(state);
}

extern "C" int32_t bf_scripting_native_codegen_supported() {
    return Luau::CodeGen::isSupported() ? 1 : 0;
}

extern "C" int32_t bf_scripting_native_codegen_enabled(lua_State *state) {
    return state != nullptr && Luau::CodeGen::isNativeExecutionEnabled(state)
        ? 1
        : 0;
}

extern "C" int32_t bf_scripting_native_codegen_compile(
    lua_State *state,
    int32_t function_index,
    int32_t type_info_level,
    BFScriptingNativeCodegenStats *out_stats) {
    if (state == nullptr || out_stats == nullptr) {
        return BF_SCRIPTING_STATUS_NULL_POINTER;
    }
    *out_stats = BFScriptingNativeCodegenStats {};
    if (type_info_level < 0 || type_info_level > 1) {
        return BF_SCRIPTING_STATUS_INVALID_ARGUMENT;
    }
    if (!Luau::CodeGen::isNativeExecutionEnabled(state)) {
        return BF_SCRIPTING_STATUS_CODEGEN_UNSUPPORTED;
    }

    try {
        const unsigned int flags = type_info_level == 0
            ? Luau::CodeGen::CodeGen_OnlyNativeModules
            : 0;
        Luau::CodeGen::CompilationStats stats {};
        const Luau::CodeGen::CompilationResult result =
            Luau::CodeGen::compile(state, function_index, flags, &stats);
        out_stats->bytecode_size_bytes = stats.bytecodeSizeBytes;
        out_stats->native_code_size_bytes = stats.nativeCodeSizeBytes;
        out_stats->native_data_size_bytes = stats.nativeDataSizeBytes;
        out_stats->native_metadata_size_bytes = stats.nativeMetadataSizeBytes;
        out_stats->functions_total = stats.functionsTotal;
        out_stats->functions_compiled = stats.functionsCompiled;
        out_stats->functions_bound = stats.functionsBound;
        out_stats->result = static_cast<int32_t>(result.result);

        switch (result.result) {
        case Luau::CodeGen::CodeGenCompilationResult::Success:
        case Luau::CodeGen::CodeGenCompilationResult::NothingToCompile:
        case Luau::CodeGen::CodeGenCompilationResult::NotNativeModule:
            return result.protoFailures.empty()
                ? BF_SCRIPTING_STATUS_OK
                : BF_SCRIPTING_STATUS_CODEGEN_FAILED;
        default:
            return BF_SCRIPTING_STATUS_CODEGEN_FAILED;
        }
    } catch (const std::bad_alloc &) {
        return BF_SCRIPTING_STATUS_OUT_OF_MEMORY;
    } catch (...) {
        return BF_SCRIPTING_STATUS_CODEGEN_FAILED;
    }
}

extern "C" int32_t bf_scripting_debugger_attach(
    lua_State *state,
    BFScriptingDebugCallback callback,
    void *callback_context,
    int32_t single_step) {
    RuntimeContext *context = runtime_context(state);
    if (context == nullptr || callback == nullptr) {
        return BF_SCRIPTING_STATUS_NULL_POINTER;
    }
    if (context->debug_callback != nullptr
        || (single_step != 0 && single_step != 1)) {
        return BF_SCRIPTING_STATUS_INVALID_ARGUMENT;
    }

    context->debug_callback = callback;
    context->debug_callback_context = callback_context;
    lua_callbacks(state)->debugbreak = debug_break;
    lua_callbacks(state)->debugstep = debug_step;
    lua_singlestep(state, single_step);

    if (Luau::CodeGen::isNativeExecutionEnabled(state)) {
        Luau::CodeGen::setNativeExecutionEnabled(state, false);
        context->native_execution_suspended = true;
    }
    return BF_SCRIPTING_STATUS_OK;
}

extern "C" void bf_scripting_debugger_detach(lua_State *state) {
    RuntimeContext *context = runtime_context(state);
    if (context == nullptr) {
        return;
    }

    lua_singlestep(state, 0);
    lua_callbacks(state)->debugbreak = nullptr;
    lua_callbacks(state)->debugstep = nullptr;
    context->debug_callback = nullptr;
    context->debug_callback_context = nullptr;
    context->resuming_breakpoint = false;
    context->resuming_step = false;
    if (context->native_execution_suspended) {
        Luau::CodeGen::setNativeExecutionEnabled(state, true);
        context->native_execution_suspended = false;
    }
}

extern "C" int32_t bf_scripting_debugger_set_breakpoint(
    lua_State *state,
    int32_t function_index,
    int32_t requested_line,
    int32_t enabled,
    int32_t *out_actual_line) {
    RuntimeContext *context = runtime_context(state);
    if (context == nullptr || out_actual_line == nullptr) {
        return BF_SCRIPTING_STATUS_NULL_POINTER;
    }
    if (context->debug_callback == nullptr
        || requested_line <= 0
        || (enabled != 0 && enabled != 1)) {
        return BF_SCRIPTING_STATUS_INVALID_ARGUMENT;
    }

    *out_actual_line =
        lua_breakpoint(state, function_index, requested_line, enabled);
    return BF_SCRIPTING_STATUS_OK;
}

extern "C" int32_t bf_scripting_compile(
    const uint8_t *source,
    size_t source_size,
    const BFScriptingCompileOptions *options,
    BFScriptingBytecode *out_bytecode) {
    if (source == nullptr || options == nullptr || out_bytecode == nullptr) {
        return BF_SCRIPTING_STATUS_NULL_POINTER;
    }
    out_bytecode->data = nullptr;
    out_bytecode->size = 0;
    if (!valid_options(*options)) {
        return BF_SCRIPTING_STATUS_INVALID_ARGUMENT;
    }

    try {
        lua_CompileOptions native_options {};
        native_options.optimizationLevel = options->optimization_level;
        native_options.debugLevel = options->debug_level;
        native_options.typeInfoLevel = options->type_info_level;
        native_options.coverageLevel = options->coverage_level;

        size_t bytecode_size = 0;
        char *bytecode = luau_compile(
            reinterpret_cast<const char *>(source),
            source_size,
            &native_options,
            &bytecode_size);
        if (bytecode == nullptr) {
            return BF_SCRIPTING_STATUS_OUT_OF_MEMORY;
        }
        out_bytecode->data = reinterpret_cast<uint8_t *>(bytecode);
        out_bytecode->size = bytecode_size;
        return BF_SCRIPTING_STATUS_OK;
    } catch (const std::bad_alloc &) {
        return BF_SCRIPTING_STATUS_OUT_OF_MEMORY;
    } catch (...) {
        return BF_SCRIPTING_STATUS_COMPILER_FAILED;
    }
}

extern "C" void bf_scripting_bytecode_free(void *bytecode) {
    std::free(bytecode);
}
