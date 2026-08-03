#ifndef BLACKFLOWER_SCRIPTING_WRAPPER_H
#define BLACKFLOWER_SCRIPTING_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

#include "lua.h"
#include "lualib.h"

#ifdef __cplusplus
extern "C" {
#endif

#define BF_SCRIPTING_STATUS_OK 0
#define BF_SCRIPTING_STATUS_NULL_POINTER 1
#define BF_SCRIPTING_STATUS_INVALID_ARGUMENT 2
#define BF_SCRIPTING_STATUS_OUT_OF_MEMORY 3
#define BF_SCRIPTING_STATUS_COMPILER_FAILED 4
#define BF_SCRIPTING_STATUS_EXECUTION_LIMIT 5
#define BF_SCRIPTING_STATUS_CODEGEN_UNSUPPORTED 6
#define BF_SCRIPTING_STATUS_CODEGEN_FAILED 7

#define BF_SCRIPTING_DEBUG_EVENT_BREAKPOINT 1
#define BF_SCRIPTING_DEBUG_EVENT_STEP 2

#define BF_SCRIPTING_DEBUG_ACTION_CONTINUE 0
#define BF_SCRIPTING_DEBUG_ACTION_STEP 1

#define BF_SCRIPTING_LIBRARY_BASE 1u
#define BF_SCRIPTING_LIBRARY_COROUTINE 2u
#define BF_SCRIPTING_LIBRARY_TABLE 4u
#define BF_SCRIPTING_LIBRARY_STRING 8u
#define BF_SCRIPTING_LIBRARY_MATH 16u
#define BF_SCRIPTING_LIBRARY_UTF8 32u
#define BF_SCRIPTING_LIBRARY_BIT32 64u
#define BF_SCRIPTING_LIBRARY_BUFFER 128u
#define BF_SCRIPTING_LIBRARY_VECTOR 256u
#define BF_SCRIPTING_LIBRARY_INTEGER 512u
#define BF_SCRIPTING_LIBRARY_ALL 1023u

typedef struct BFScriptingVersion {
    uint32_t major;
    uint32_t minor;
    uint32_t patch;
} BFScriptingVersion;

typedef struct BFScriptingCompileOptions {
    int32_t optimization_level;
    int32_t debug_level;
    int32_t type_info_level;
    int32_t coverage_level;
} BFScriptingCompileOptions;

typedef struct BFScriptingBytecode {
    uint8_t *data;
    size_t size;
} BFScriptingBytecode;

typedef struct BFScriptingRuntime {
    lua_State *state;
    void *context;
} BFScriptingRuntime;

typedef int32_t (*BFScriptingDebugCallback)(
    void *context,
    lua_State *state,
    int32_t event_kind);

typedef struct BFScriptingMemoryUsage {
    size_t current_bytes;
    size_t peak_bytes;
    size_t limit_bytes;
} BFScriptingMemoryUsage;

typedef struct BFScriptingBytesView {
    const uint8_t *data;
    size_t size;
} BFScriptingBytesView;

typedef struct BFScriptingNativeCodegenStats {
    size_t bytecode_size_bytes;
    size_t native_code_size_bytes;
    size_t native_data_size_bytes;
    size_t native_metadata_size_bytes;
    uint32_t functions_total;
    uint32_t functions_compiled;
    uint32_t functions_bound;
    int32_t result;
} BFScriptingNativeCodegenStats;

BFScriptingVersion bf_scripting_luau_version(void);

int32_t bf_scripting_runtime_new(
    size_t memory_limit_bytes,
    size_t native_codegen_limit_bytes,
    BFScriptingRuntime *out_runtime);

void bf_scripting_runtime_free(BFScriptingRuntime *runtime);

BFScriptingMemoryUsage bf_scripting_runtime_memory_usage(
    const BFScriptingRuntime *runtime);

BFScriptingMemoryUsage bf_scripting_runtime_native_codegen_memory_usage(
    const BFScriptingRuntime *runtime);

BFScriptingBytesView bf_scripting_runtime_last_debug_trace(
    const BFScriptingRuntime *runtime);

int32_t bf_scripting_initialize(
    lua_State *state,
    int32_t random_seed,
    uint32_t libraries);

int32_t bf_scripting_begin_execution(lua_State *state, uint64_t fuel);

int32_t bf_scripting_end_execution(lua_State *state);

int32_t bf_scripting_pcall(
    lua_State *state,
    int32_t argument_count,
    int32_t result_count);

void bf_scripting_capture_debug_trace(lua_State *state);

int32_t bf_scripting_native_codegen_supported(void);

int32_t bf_scripting_native_codegen_enabled(lua_State *state);

int32_t bf_scripting_native_codegen_compile(
    lua_State *state,
    int32_t function_index,
    int32_t type_info_level,
    BFScriptingNativeCodegenStats *out_stats);

int32_t bf_scripting_debugger_attach(
    lua_State *state,
    BFScriptingDebugCallback callback,
    void *callback_context,
    int32_t single_step);

void bf_scripting_debugger_detach(lua_State *state);

int32_t bf_scripting_debugger_set_breakpoint(
    lua_State *state,
    int32_t function_index,
    int32_t requested_line,
    int32_t enabled,
    int32_t *out_actual_line);

int32_t bf_scripting_compile(
    const uint8_t *source,
    size_t source_size,
    const BFScriptingCompileOptions *options,
    BFScriptingBytecode *out_bytecode);

void bf_scripting_bytecode_free(void *bytecode);

#ifdef __cplusplus
}
#endif

#endif
