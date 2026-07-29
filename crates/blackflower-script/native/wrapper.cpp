#include "wrapper.h"

#include "luacode.h"

#include <cstdlib>
#include <new>

namespace {

struct InitializeContext {
    int32_t random_seed;
};

void open_library(lua_State *state, const char *name, lua_CFunction function) {
    lua_pushcfunction(state, function, nullptr);
    lua_pushstring(state, name);
    lua_call(state, 1, 0);
}

int initialize_runtime(lua_State *state) {
    const auto *context =
        static_cast<const InitializeContext *>(lua_touserdata(state, 1));

    open_library(state, "", luaopen_base);
    open_library(state, LUA_COLIBNAME, luaopen_coroutine);
    open_library(state, LUA_TABLIBNAME, luaopen_table);
    open_library(state, LUA_STRLIBNAME, luaopen_string);
    open_library(state, LUA_MATHLIBNAME, luaopen_math);
    open_library(state, LUA_UTF8LIBNAME, luaopen_utf8);
    open_library(state, LUA_BITLIBNAME, luaopen_bit32);
    open_library(state, LUA_BUFFERLIBNAME, luaopen_buffer);
    open_library(state, LUA_VECLIBNAME, luaopen_vector);
    open_library(state, LUA_INTLIBNAME, luaopen_integer);

    lua_getglobal(state, LUA_MATHLIBNAME);
    lua_getfield(state, -1, "randomseed");
    lua_pushinteger(state, context->random_seed);
    lua_call(state, 1, 0);
    lua_pop(state, 1);

    luaL_sandbox(state);
    luaL_sandboxthread(state);
    lua_pushvalue(state, LUA_GLOBALSINDEX);
    lua_setfield(state, -1, "_G");
    return 0;
}

bool valid_options(const BFScriptCompileOptions &options) {
    return options.optimization_level >= 0 && options.optimization_level <= 2
        && options.debug_level >= 0 && options.debug_level <= 2
        && options.type_info_level >= 0 && options.type_info_level <= 1
        && options.coverage_level >= 0 && options.coverage_level <= 2;
}

} // namespace

extern "C" BFScriptVersion bf_script_luau_version() {
    return BFScriptVersion {
        BF_LUAU_VERSION_MAJOR,
        BF_LUAU_VERSION_MINOR,
        BF_LUAU_VERSION_PATCH,
    };
}

extern "C" int32_t bf_script_initialize(lua_State *state, int32_t random_seed) {
    if (state == nullptr) {
        return LUA_ERRRUN;
    }
    InitializeContext context {random_seed};
    return lua_cpcall(state, initialize_runtime, &context);
}

extern "C" int32_t bf_script_compile(
    const uint8_t *source,
    size_t source_size,
    const BFScriptCompileOptions *options,
    BFScriptBytecode *out_bytecode) {
    if (source == nullptr || options == nullptr || out_bytecode == nullptr) {
        return BF_SCRIPT_STATUS_NULL_POINTER;
    }
    out_bytecode->data = nullptr;
    out_bytecode->size = 0;
    if (!valid_options(*options)) {
        return BF_SCRIPT_STATUS_INVALID_ARGUMENT;
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
            return BF_SCRIPT_STATUS_OUT_OF_MEMORY;
        }
        out_bytecode->data = reinterpret_cast<uint8_t *>(bytecode);
        out_bytecode->size = bytecode_size;
        return BF_SCRIPT_STATUS_OK;
    } catch (const std::bad_alloc &) {
        return BF_SCRIPT_STATUS_OUT_OF_MEMORY;
    } catch (...) {
        return BF_SCRIPT_STATUS_COMPILER_FAILED;
    }
}

extern "C" void bf_script_bytecode_free(void *bytecode) {
    std::free(bytecode);
}
