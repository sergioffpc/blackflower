#ifndef BLACKFLOWER_SHADER_COMPILER_WRAPPER_H
#define BLACKFLOWER_SHADER_COMPILER_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BF_SHADER_COMPILER_STATUS_OK 0
#define BF_SHADER_COMPILER_STATUS_NULL_POINTER 1
#define BF_SHADER_COMPILER_STATUS_INVALID_ARGUMENT 2
#define BF_SHADER_COMPILER_STATUS_OUT_OF_MEMORY 3
#define BF_SHADER_COMPILER_STATUS_INITIALIZATION_FAILED 4
#define BF_SHADER_COMPILER_STATUS_COMPILATION_FAILED 5

#define BF_SHADER_STAGE_VERTEX 1
#define BF_SHADER_STAGE_FRAGMENT 2
#define BF_SHADER_STAGE_COMPUTE 3

typedef struct BFShaderCompilerOptions {
    int32_t stage;
    int32_t optimization;
    int32_t debug_info;
} BFShaderCompilerOptions;

typedef struct BFShaderCompilerBlob {
    uint8_t *data;
    size_t size;
} BFShaderCompilerBlob;

const char *bf_shader_compiler_slang_version(void);

int32_t bf_shader_compiler_compile_spirv(
    const uint8_t *source_name,
    size_t source_name_size,
    const uint8_t *source,
    size_t source_size,
    const uint8_t *entry_point,
    size_t entry_point_size,
    const BFShaderCompilerOptions *options,
    BFShaderCompilerBlob *out_spirv,
    BFShaderCompilerBlob *out_diagnostics);

void bf_shader_compiler_blob_free(void *data);

#ifdef __cplusplus
}
#endif

#endif
