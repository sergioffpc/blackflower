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

BFScriptVersion bf_script_luau_version(void);

int32_t bf_script_initialize(lua_State *state, int32_t random_seed);

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
