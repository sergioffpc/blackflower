#include "wrapper.h"

#include "ktx.h"
#include "vkformat_enum.h"

#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <new>

namespace {

constexpr char SEMANTIC_KEY[] = "BlackflowerSemantic";
constexpr char ORIENTATION[] = "rd";
constexpr uint32_t RGBA8_BYTES_PER_TEXEL = 4;
constexpr uint32_t RGBA16F_BYTES_PER_TEXEL = 8;

class TextureOwner {
public:
    explicit TextureOwner(ktxTexture2 *texture = nullptr) : texture_(texture) {}

    ~TextureOwner() {
        if (texture_ != nullptr) {
            ktxTexture2_Destroy(texture_);
        }
    }

    TextureOwner(const TextureOwner &) = delete;
    TextureOwner &operator=(const TextureOwner &) = delete;

    ktxTexture2 *get() const { return texture_; }

private:
    ktxTexture2 *texture_;
};

int32_t ktx_status(KTX_error_code status) {
    if (status == KTX_SUCCESS) {
        return BF_TEXTURE_STATUS_OK;
    }
    return BF_TEXTURE_STATUS_KTX_ERROR_BASE + static_cast<int32_t>(status);
}

bool valid_semantic(int32_t semantic) {
    return semantic >= BF_TEXTURE_SEMANTIC_COLOR_SRGB
        && semantic <= BF_TEXTURE_SEMANTIC_HDR_LINEAR;
}

bool valid_quality(int32_t quality) {
    return quality == BF_TEXTURE_QUALITY_FAST
        || quality == BF_TEXTURE_QUALITY_HIGH;
}

const char *semantic_name(int32_t semantic) {
    switch (semantic) {
    case BF_TEXTURE_SEMANTIC_COLOR_SRGB:
        return "color_srgb";
    case BF_TEXTURE_SEMANTIC_NORMAL_LINEAR:
        return "normal_linear";
    case BF_TEXTURE_SEMANTIC_DATA_LINEAR:
        return "data_linear";
    case BF_TEXTURE_SEMANTIC_HDR_LINEAR:
        return "hdr_linear";
    default:
        return nullptr;
    }
}

int32_t semantic_value(const char *value, size_t size) {
    if (value == nullptr || size == 0) {
        return 0;
    }
    const size_t text_size = value[size - 1] == '\0' ? size - 1 : size;
    for (int32_t semantic = BF_TEXTURE_SEMANTIC_COLOR_SRGB;
         semantic <= BF_TEXTURE_SEMANTIC_HDR_LINEAR;
         ++semantic) {
        const char *candidate = semantic_name(semantic);
        const size_t candidate_size = std::strlen(candidate);
        if (candidate_size == text_size
            && std::memcmp(candidate, value, text_size) == 0) {
            return semantic;
        }
    }
    return 0;
}

int32_t add_metadata(ktxTexture2 *texture, int32_t semantic) {
    const char *value = semantic_name(semantic);
    if (value == nullptr) {
        return BF_TEXTURE_STATUS_INVALID_ARGUMENT;
    }
    KTX_error_code status = ktxHashList_AddKVPair(
        &texture->kvDataHead,
        SEMANTIC_KEY,
        static_cast<unsigned int>(std::strlen(value) + 1),
        value);
    if (status != KTX_SUCCESS) {
        return ktx_status(status);
    }
    status = ktxHashList_AddKVPair(
        &texture->kvDataHead,
        KTX_ORIENTATION_KEY,
        static_cast<unsigned int>(sizeof(ORIENTATION)),
        ORIENTATION);
    if (status != KTX_SUCCESS) {
        return ktx_status(status);
    }
    return ktx_status(ktxHashList_Sort(&texture->kvDataHead));
}

int32_t read_semantic(ktxTexture2 *texture) {
    unsigned int value_size = 0;
    void *value = nullptr;
    const KTX_error_code status = ktxHashList_FindValue(
        &texture->kvDataHead,
        SEMANTIC_KEY,
        &value_size,
        &value);
    if (status != KTX_SUCCESS || value == nullptr) {
        return 0;
    }
    return semantic_value(
        static_cast<const char *>(value),
        static_cast<size_t>(value_size));
}

bool validate_texture(ktxTexture2 *texture, int32_t *semantic) {
    if (texture == nullptr
        || texture->numDimensions != 2
        || texture->baseWidth == 0
        || texture->baseHeight == 0
        || texture->baseDepth != 1
        || texture->isArray
        || texture->isCubemap
        || texture->numLayers != 1
        || texture->numFaces != 1
        || texture->numLevels == 0
        || texture->numLevels > BF_TEXTURE_MAX_LEVELS) {
        return false;
    }
    *semantic = read_semantic(texture);
    if (!valid_semantic(*semantic)) {
        return false;
    }
    const bool transcodable =
        ktxTexture_NeedsTranscoding(ktxTexture(texture)) == KTX_TRUE;
    if (*semantic == BF_TEXTURE_SEMANTIC_HDR_LINEAR) {
        return !transcodable
            && texture->vkFormat == VK_FORMAT_R16G16B16A16_SFLOAT;
    }
    return transcodable;
}

bool expected_level(
    const BFTextureSourceLevel &level,
    uint32_t base_width,
    uint32_t base_height,
    size_t index,
    uint32_t bytes_per_texel) {
    const uint32_t shift = static_cast<uint32_t>(index);
    const uint32_t width = std::max(base_width >> shift, 1U);
    const uint32_t height = std::max(base_height >> shift, 1U);
    if (level.data == nullptr
        || level.width != width
        || level.height != height) {
        return false;
    }
    const uint64_t required = static_cast<uint64_t>(width)
        * static_cast<uint64_t>(height)
        * bytes_per_texel;
    return required <= std::numeric_limits<size_t>::max()
        && level.size == static_cast<size_t>(required);
}

int32_t serialize(ktxTexture2 *texture, BFTextureBlob *output) {
    uint8_t *bytes = nullptr;
    ktx_size_t size = 0;
    const KTX_error_code status =
        ktxTexture_WriteToMemory(ktxTexture(texture), &bytes, &size);
    if (status != KTX_SUCCESS) {
        return ktx_status(status);
    }
    if (bytes == nullptr || size == 0) {
        std::free(bytes);
        return BF_TEXTURE_STATUS_INVALID_KTX2;
    }
    output->data = static_cast<uint8_t *>(std::malloc(size));
    if (output->data == nullptr) {
        std::free(bytes);
        return BF_TEXTURE_STATUS_OUT_OF_MEMORY;
    }
    std::memcpy(output->data, bytes, size);
    output->size = size;
    std::free(bytes);
    return BF_TEXTURE_STATUS_OK;
}

int32_t create_from_memory(
    const uint8_t *bytes,
    size_t size,
    TextureOwner *owner,
    ktxTexture2 **texture) {
    if (bytes == nullptr || size == 0 || owner == nullptr || texture == nullptr) {
        return BF_TEXTURE_STATUS_NULL_POINTER;
    }
    ktxTexture2 *created = nullptr;
    const KTX_error_code status = ktxTexture2_CreateFromMemory(
        bytes,
        size,
        KTX_TEXTURE_CREATE_LOAD_IMAGE_DATA_BIT,
        &created);
    if (status != KTX_SUCCESS || created == nullptr) {
        return BF_TEXTURE_STATUS_INVALID_KTX2;
    }
    new (owner) TextureOwner(created);
    *texture = created;
    return BF_TEXTURE_STATUS_OK;
}

bool target_format(
    int32_t requested,
    int32_t semantic,
    ktx_transcode_fmt_e *native,
    uint32_t *vk_format) {
    const bool srgb = semantic == BF_TEXTURE_SEMANTIC_COLOR_SRGB;
    switch (requested) {
    case BF_TEXTURE_FORMAT_RGBA8:
        *native = KTX_TTF_RGBA32;
        *vk_format = srgb
            ? VK_FORMAT_R8G8B8A8_SRGB
            : VK_FORMAT_R8G8B8A8_UNORM;
        return true;
    case BF_TEXTURE_FORMAT_BC7_RGBA:
        *native = KTX_TTF_BC7_RGBA;
        *vk_format = srgb
            ? VK_FORMAT_BC7_SRGB_BLOCK
            : VK_FORMAT_BC7_UNORM_BLOCK;
        return true;
    case BF_TEXTURE_FORMAT_BC5_RG:
        *native = KTX_TTF_BC5_RG;
        *vk_format = VK_FORMAT_BC5_UNORM_BLOCK;
        return true;
    case BF_TEXTURE_FORMAT_ASTC_4X4_RGBA:
        *native = KTX_TTF_ASTC_4x4_RGBA;
        *vk_format = srgb
            ? VK_FORMAT_ASTC_4x4_SRGB_BLOCK
            : VK_FORMAT_ASTC_4x4_UNORM_BLOCK;
        return true;
    case BF_TEXTURE_FORMAT_ETC2_RGBA:
        *native = KTX_TTF_ETC2_RGBA;
        *vk_format = srgb
            ? VK_FORMAT_ETC2_R8G8B8A8_SRGB_BLOCK
            : VK_FORMAT_ETC2_R8G8B8A8_UNORM_BLOCK;
        return true;
    case BF_TEXTURE_FORMAT_ETC2_EAC_RG11:
        *native = KTX_TTF_ETC2_EAC_RG11;
        *vk_format = VK_FORMAT_EAC_R11G11_UNORM_BLOCK;
        return true;
    default:
        return false;
    }
}

int32_t copy_levels(
    ktxTexture2 *texture,
    int32_t semantic,
    int32_t format,
    BFTranscodedTexture *output) {
    size_t total_size = 0;
    size_t sizes[BF_TEXTURE_MAX_LEVELS] = {};
    size_t source_offsets[BF_TEXTURE_MAX_LEVELS] = {};
    for (uint32_t level = 0; level < texture->numLevels; ++level) {
        ktx_size_t source_offset = 0;
        const KTX_error_code status = ktxTexture2_GetImageOffset(
            texture,
            level,
            0,
            0,
            &source_offset);
        if (status != KTX_SUCCESS) {
            return ktx_status(status);
        }
        const ktx_size_t level_size =
            ktxTexture_GetImageSize(ktxTexture(texture), level);
        if (level_size > std::numeric_limits<size_t>::max() - total_size) {
            return BF_TEXTURE_STATUS_OUT_OF_MEMORY;
        }
        source_offsets[level] = source_offset;
        sizes[level] = level_size;
        total_size += level_size;
    }

    uint8_t *data = static_cast<uint8_t *>(std::malloc(total_size));
    if (data == nullptr && total_size != 0) {
        return BF_TEXTURE_STATUS_OUT_OF_MEMORY;
    }
    const uint8_t *source = ktxTexture_GetData(ktxTexture(texture));
    if (source == nullptr && total_size != 0) {
        std::free(data);
        return BF_TEXTURE_STATUS_INVALID_KTX2;
    }

    size_t destination_offset = 0;
    for (uint32_t level = 0; level < texture->numLevels; ++level) {
        std::memcpy(
            data + destination_offset,
            source + source_offsets[level],
            sizes[level]);
        output->levels[level].offset = destination_offset;
        output->levels[level].size = sizes[level];
        output->levels[level].width =
            std::max(texture->baseWidth >> level, 1U);
        output->levels[level].height =
            std::max(texture->baseHeight >> level, 1U);
        destination_offset += sizes[level];
    }

    output->bytes.data = data;
    output->bytes.size = total_size;
    output->level_count = texture->numLevels;
    output->width = texture->baseWidth;
    output->height = texture->baseHeight;
    output->semantic = semantic;
    output->format = format;
    return BF_TEXTURE_STATUS_OK;
}

} // namespace

extern "C" const char *bf_texture_ktx_version() {
    return BF_KTX_VERSION;
}

extern "C" const char *bf_texture_status_message(int32_t status) {
    switch (status) {
    case BF_TEXTURE_STATUS_OK:
        return "success";
    case BF_TEXTURE_STATUS_NULL_POINTER:
        return "null pointer passed to texture boundary";
    case BF_TEXTURE_STATUS_INVALID_ARGUMENT:
        return "invalid texture argument";
    case BF_TEXTURE_STATUS_OUT_OF_MEMORY:
        return "native texture allocation failed";
    case BF_TEXTURE_STATUS_INVALID_KTX2:
        return "invalid or unsupported Blackflower KTX2 texture";
    case BF_TEXTURE_STATUS_UNSUPPORTED:
        return "unsupported texture format or transcode";
    default:
        if (status >= BF_TEXTURE_STATUS_KTX_ERROR_BASE
            && status <= BF_TEXTURE_STATUS_KTX_ERROR_BASE + KTX_ERROR_MAX_ENUM) {
            return ktxErrorString(
                static_cast<KTX_error_code>(
                    status - BF_TEXTURE_STATUS_KTX_ERROR_BASE));
        }
        return "unknown native texture status";
    }
}

extern "C" int32_t bf_texture_encode(
    const BFTextureSourceLevel *levels,
    size_t level_count,
    const BFTextureEncodeOptions *options,
    BFTextureBlob *out_ktx2) {
    if (levels == nullptr || options == nullptr || out_ktx2 == nullptr) {
        return BF_TEXTURE_STATUS_NULL_POINTER;
    }
    out_ktx2->data = nullptr;
    out_ktx2->size = 0;
    if (level_count == 0
        || level_count > BF_TEXTURE_MAX_LEVELS
        || !valid_semantic(options->semantic)
        || !valid_quality(options->quality)
        || options->zstd_level == 0
        || options->zstd_level > 22
        || levels[0].width == 0
        || levels[0].height == 0) {
        return BF_TEXTURE_STATUS_INVALID_ARGUMENT;
    }

    const bool hdr = options->semantic == BF_TEXTURE_SEMANTIC_HDR_LINEAR;
    const uint32_t bytes_per_texel =
        hdr ? RGBA16F_BYTES_PER_TEXEL : RGBA8_BYTES_PER_TEXEL;
    for (size_t index = 0; index < level_count; ++index) {
        if (!expected_level(
                levels[index],
                levels[0].width,
                levels[0].height,
                index,
                bytes_per_texel)) {
            return BF_TEXTURE_STATUS_INVALID_ARGUMENT;
        }
    }

    ktxTextureCreateInfo create_info = {};
    create_info.vkFormat = hdr
        ? VK_FORMAT_R16G16B16A16_SFLOAT
        : options->semantic == BF_TEXTURE_SEMANTIC_COLOR_SRGB
            ? VK_FORMAT_R8G8B8A8_SRGB
            : VK_FORMAT_R8G8B8A8_UNORM;
    create_info.baseWidth = levels[0].width;
    create_info.baseHeight = levels[0].height;
    create_info.baseDepth = 1;
    create_info.numDimensions = 2;
    create_info.numLevels = static_cast<uint32_t>(level_count);
    create_info.numLayers = 1;
    create_info.numFaces = 1;
    create_info.isArray = KTX_FALSE;
    create_info.generateMipmaps = KTX_FALSE;

    ktxTexture2 *created = nullptr;
    KTX_error_code status = ktxTexture2_Create(
        &create_info,
        KTX_TEXTURE_CREATE_ALLOC_STORAGE,
        &created);
    if (status != KTX_SUCCESS || created == nullptr) {
        return ktx_status(status);
    }
    TextureOwner texture(created);

    for (uint32_t level = 0; level < level_count; ++level) {
        status = ktxTexture_SetImageFromMemory(
            ktxTexture(texture.get()),
            level,
            0,
            0,
            levels[level].data,
            levels[level].size);
        if (status != KTX_SUCCESS) {
            return ktx_status(status);
        }
    }
    int32_t wrapper_status = add_metadata(texture.get(), options->semantic);
    if (wrapper_status != BF_TEXTURE_STATUS_OK) {
        return wrapper_status;
    }

    if (!hdr) {
        ktxBasisParams params = {};
        params.structSize = sizeof(params);
        params.uastc = KTX_TRUE;
        params.threadCount = 1;
        params.compressionLevel = KTX_ETC1S_DEFAULT_COMPRESSION_LEVEL;
        params.uastcFlags = options->quality == BF_TEXTURE_QUALITY_FAST
            ? KTX_PACK_UASTC_LEVEL_FASTER
            : KTX_PACK_UASTC_LEVEL_SLOWER;
        params.uastcRDO = options->uastc_rdo ? KTX_TRUE : KTX_FALSE;
        params.uastcRDONoMultithreading = KTX_TRUE;
        params.normalMap =
            options->semantic == BF_TEXTURE_SEMANTIC_NORMAL_LINEAR
                ? KTX_TRUE
                : KTX_FALSE;
        status = ktxTexture2_CompressBasisEx(texture.get(), &params);
        if (status != KTX_SUCCESS) {
            return ktx_status(status);
        }
    }

    status = ktxTexture2_DeflateZstd(texture.get(), options->zstd_level);
    if (status != KTX_SUCCESS) {
        return ktx_status(status);
    }
    return serialize(texture.get(), out_ktx2);
}

extern "C" int32_t bf_texture_inspect(
    const uint8_t *ktx2,
    size_t ktx2_size,
    BFTextureInfo *out_info) {
    if (ktx2 == nullptr || out_info == nullptr) {
        return BF_TEXTURE_STATUS_NULL_POINTER;
    }
    std::memset(out_info, 0, sizeof(*out_info));
    alignas(TextureOwner) unsigned char owner_storage[sizeof(TextureOwner)];
    auto *owner = reinterpret_cast<TextureOwner *>(owner_storage);
    ktxTexture2 *texture = nullptr;
    const int32_t status =
        create_from_memory(ktx2, ktx2_size, owner, &texture);
    if (status != BF_TEXTURE_STATUS_OK) {
        return status;
    }
    int32_t semantic = 0;
    const bool valid = validate_texture(texture, &semantic);
    const uint32_t width = texture->baseWidth;
    const uint32_t height = texture->baseHeight;
    const uint32_t levels = texture->numLevels;
    const uint8_t needs_transcoding =
        ktxTexture_NeedsTranscoding(ktxTexture(texture)) == KTX_TRUE ? 1 : 0;
    owner->~TextureOwner();
    if (!valid) {
        return BF_TEXTURE_STATUS_INVALID_KTX2;
    }

    out_info->width = width;
    out_info->height = height;
    out_info->levels = levels;
    out_info->semantic = semantic;
    out_info->needs_transcoding = needs_transcoding;
    return BF_TEXTURE_STATUS_OK;
}

extern "C" int32_t bf_texture_transcode(
    const uint8_t *ktx2,
    size_t ktx2_size,
    int32_t target_format_value,
    BFTranscodedTexture *out_texture) {
    if (ktx2 == nullptr || out_texture == nullptr) {
        return BF_TEXTURE_STATUS_NULL_POINTER;
    }
    std::memset(out_texture, 0, sizeof(*out_texture));
    alignas(TextureOwner) unsigned char owner_storage[sizeof(TextureOwner)];
    auto *owner = reinterpret_cast<TextureOwner *>(owner_storage);
    ktxTexture2 *texture = nullptr;
    int32_t status = create_from_memory(ktx2, ktx2_size, owner, &texture);
    if (status != BF_TEXTURE_STATUS_OK) {
        return status;
    }

    int32_t semantic = 0;
    if (!validate_texture(texture, &semantic)) {
        owner->~TextureOwner();
        return BF_TEXTURE_STATUS_INVALID_KTX2;
    }

    const bool needs_transcoding =
        ktxTexture_NeedsTranscoding(ktxTexture(texture)) == KTX_TRUE;
    if (target_format_value == BF_TEXTURE_FORMAT_RGBA16_FLOAT) {
        if (needs_transcoding
            || semantic != BF_TEXTURE_SEMANTIC_HDR_LINEAR
            || texture->vkFormat != VK_FORMAT_R16G16B16A16_SFLOAT) {
            owner->~TextureOwner();
            return BF_TEXTURE_STATUS_UNSUPPORTED;
        }
    } else {
        ktx_transcode_fmt_e target = KTX_TTF_NOSELECTION;
        uint32_t expected_vk_format = 0;
        if (!needs_transcoding
            || !target_format(
                target_format_value,
                semantic,
                &target,
                &expected_vk_format)) {
            owner->~TextureOwner();
            return BF_TEXTURE_STATUS_UNSUPPORTED;
        }
        const KTX_error_code transcode_status =
            ktxTexture2_TranscodeBasis(texture, target, 0);
        if (transcode_status != KTX_SUCCESS
            || texture->vkFormat != expected_vk_format) {
            owner->~TextureOwner();
            return transcode_status == KTX_SUCCESS
                ? BF_TEXTURE_STATUS_INVALID_KTX2
                : ktx_status(transcode_status);
        }
    }

    status = copy_levels(
        texture,
        semantic,
        target_format_value,
        out_texture);
    owner->~TextureOwner();
    return status;
}

extern "C" void bf_texture_blob_free(void *data) {
    std::free(data);
}
