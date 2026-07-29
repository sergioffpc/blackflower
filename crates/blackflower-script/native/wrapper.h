#ifndef BLACKFLOWER_SCRIPT_WRAPPER_H
#define BLACKFLOWER_SCRIPT_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

#include "lua.h"
#include "lualib.h"

#ifdef __cplusplus
extern "C" {
#endif

#define BF_SCRIPT_STATUS_OK 0
#define BF_SCRIPT_STATUS_NULL_POINTER 1
#define BF_SCRIPT_STATUS_INVALID_ARGUMENT 2
#define BF_SCRIPT_STATUS_OUT_OF_MEMORY 3
#define BF_SCRIPT_STATUS_COMPILER_FAILED 4
#define BF_SCRIPT_STATUS_EXECUTION_LIMIT 5

#define BF_SCRIPT_LIBRARY_BASE 1u
#define BF_SCRIPT_LIBRARY_COROUTINE 2u
#define BF_SCRIPT_LIBRARY_TABLE 4u
#define BF_SCRIPT_LIBRARY_STRING 8u
#define BF_SCRIPT_LIBRARY_MATH 16u
#define BF_SCRIPT_LIBRARY_UTF8 32u
#define BF_SCRIPT_LIBRARY_BIT32 64u
#define BF_SCRIPT_LIBRARY_BUFFER 128u
#define BF_SCRIPT_LIBRARY_VECTOR 256u
#define BF_SCRIPT_LIBRARY_INTEGER 512u
#define BF_SCRIPT_LIBRARY_ALL 1023u

typedef struct BFScriptVersion {
    uint32_t major;
    uint32_t minor;
    uint32_t patch;
} BFScriptVersion;

typedef struct BFScriptCompileOptions {
    int32_t optimization_level;
    int32_t debug_level;
    int32_t type_info_level;
    int32_t coverage_level;
} BFScriptCompileOptions;

typedef struct BFScriptBytecode {
    uint8_t *data;
    size_t size;
} BFScriptBytecode;

typedef struct BFScriptRuntime {
    lua_State *state;
    void *context;
} BFScriptRuntime;

typedef struct BFScriptMemoryUsage {
    size_t current_bytes;
    size_t peak_bytes;
    size_t limit_bytes;
} BFScriptMemoryUsage;

BFScriptVersion bf_script_luau_version(void);

int32_t bf_script_runtime_new(
    size_t memory_limit_bytes,
    BFScriptRuntime *out_runtime);

void bf_script_runtime_free(BFScriptRuntime *runtime);

BFScriptMemoryUsage bf_script_runtime_memory_usage(
    const BFScriptRuntime *runtime);

int32_t bf_script_initialize(
    lua_State *state,
    int32_t random_seed,
    uint32_t libraries);

int32_t bf_script_begin_execution(lua_State *state, uint64_t fuel);

int32_t bf_script_end_execution(lua_State *state);

int32_t bf_script_compile(
    const uint8_t *source,
    size_t source_size,
    const BFScriptCompileOptions *options,
    BFScriptBytecode *out_bytecode);

void bf_script_bytecode_free(void *bytecode);

#ifdef __cplusplus
}
#endif

#endif
