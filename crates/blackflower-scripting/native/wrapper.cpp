#include "wrapper.h"

#include "luacode.h"

#include <algorithm>
#include <cstdlib>
#include <new>

namespace {

struct RuntimeContext {
    size_t current_bytes;
    size_t peak_bytes;
    size_t limit_bytes;
    uint64_t remaining_fuel;
    bool execution_active;
    bool execution_limit_reached;
};

struct InitializeContext {
    int32_t random_seed;
    uint32_t libraries;
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
        }
    }

    if ((context->libraries & BF_SCRIPTING_LIBRARY_MATH) != 0) {
        lua_getglobal(state, LUA_MATHLIBNAME);
        lua_getfield(state, -1, "randomseed");
        lua_pushinteger(state, context->random_seed);
        lua_call(state, 1, 0);
        lua_pop(state, 1);
    }

    luaL_sandbox(state);
    luaL_sandboxthread(state);
    lua_pushvalue(state, LUA_GLOBALSINDEX);
    lua_setfield(state, -1, "_G");
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
        false,
        false,
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
    lua_callbacks(state)->interrupt = execution_interrupt;
    return BF_SCRIPTING_STATUS_OK;
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
