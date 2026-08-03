#ifndef BLACKFLOWER_TEXTURE_WRAPPER_H
#define BLACKFLOWER_TEXTURE_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BF_TEXTURE_STATUS_OK 0
#define BF_TEXTURE_STATUS_NULL_POINTER 1
#define BF_TEXTURE_STATUS_INVALID_ARGUMENT 2
#define BF_TEXTURE_STATUS_OUT_OF_MEMORY 3
#define BF_TEXTURE_STATUS_INVALID_KTX2 4
#define BF_TEXTURE_STATUS_UNSUPPORTED 5
#define BF_TEXTURE_STATUS_KTX_ERROR_BASE 100

#define BF_TEXTURE_SEMANTIC_COLOR_SRGB 1
#define BF_TEXTURE_SEMANTIC_NORMAL_LINEAR 2
#define BF_TEXTURE_SEMANTIC_DATA_LINEAR 3
#define BF_TEXTURE_SEMANTIC_HDR_LINEAR 4

#define BF_TEXTURE_QUALITY_FAST 1
#define BF_TEXTURE_QUALITY_HIGH 2

#define BF_TEXTURE_FORMAT_RGBA8 1
#define BF_TEXTURE_FORMAT_RGBA16_FLOAT 2
#define BF_TEXTURE_FORMAT_BC7_RGBA 3
#define BF_TEXTURE_FORMAT_BC5_RG 4
#define BF_TEXTURE_FORMAT_ASTC_4X4_RGBA 5
#define BF_TEXTURE_FORMAT_ETC2_RGBA 6
#define BF_TEXTURE_FORMAT_ETC2_EAC_RG11 7

#define BF_TEXTURE_MAX_LEVELS 32

typedef struct BFTextureBlob {
    uint8_t *data;
    size_t size;
} BFTextureBlob;

typedef struct BFTextureSourceLevel {
    const uint8_t *data;
    size_t size;
    uint32_t width;
    uint32_t height;
} BFTextureSourceLevel;

typedef struct BFTextureEncodeOptions {
    int32_t semantic;
    int32_t quality;
    uint32_t zstd_level;
    uint8_t uastc_rdo;
} BFTextureEncodeOptions;

typedef struct BFTextureInfo {
    uint32_t width;
    uint32_t height;
    uint32_t levels;
    int32_t semantic;
    uint8_t needs_transcoding;
} BFTextureInfo;

typedef struct BFTextureLevelLayout {
    size_t offset;
    size_t size;
    uint32_t width;
    uint32_t height;
} BFTextureLevelLayout;

typedef struct BFTranscodedTexture {
    BFTextureBlob bytes;
    BFTextureLevelLayout levels[BF_TEXTURE_MAX_LEVELS];
    uint32_t level_count;
    uint32_t width;
    uint32_t height;
    int32_t semantic;
    int32_t format;
} BFTranscodedTexture;

const char *bf_texture_ktx_version(void);
const char *bf_texture_status_message(int32_t status);

int32_t bf_texture_encode(
    const BFTextureSourceLevel *levels,
    size_t level_count,
    const BFTextureEncodeOptions *options,
    BFTextureBlob *out_ktx2);

int32_t bf_texture_inspect(
    const uint8_t *ktx2,
    size_t ktx2_size,
    BFTextureInfo *out_info);

int32_t bf_texture_transcode(
    const uint8_t *ktx2,
    size_t ktx2_size,
    int32_t target_format,
    BFTranscodedTexture *out_texture);

void bf_texture_blob_free(void *data);

#ifdef __cplusplus
}
#endif

#endif
