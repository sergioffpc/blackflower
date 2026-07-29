#include "wrapper.h"

#include <nanovdb/GridHandle.h>
#include <nanovdb/NanoVDB.h>
#include <nanovdb/io/IO.h>
#include <nanovdb/math/SampleFromVoxels.h>
#include <nanovdb/tools/GridValidator.h>

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <limits>
#include <memory>
#include <new>
#include <sstream>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

struct BFRenderingVolumesNanoVdb {
    std::vector<nanovdb::GridHandle<nanovdb::HostBuffer>> handles;
    std::vector<const nanovdb::GridData *> grids;
};

namespace {

constexpr std::size_t GRID_TYPE_OFFSET = 636;
constexpr std::size_t GRID_CLASS_OFFSET = 632;
constexpr std::size_t MINIMUM_GRID_SIZE =
    sizeof(nanovdb::GridData) + sizeof(nanovdb::TreeData) + NANOVDB_DATA_ALIGNMENT;

struct RawGridHeader {
    std::uint64_t magic;
    std::uint64_t checksum;
    std::uint32_t version;
    std::uint32_t flags;
    std::uint32_t grid_index;
    std::uint32_t grid_count;
    std::uint64_t grid_size;
};

struct RawFileHeader {
    std::uint64_t magic;
    std::uint32_t version;
    std::uint16_t grid_count;
    std::uint16_t codec;
};

struct RawFileMetadata {
    std::uint64_t grid_size;
    std::uint64_t file_size;
    std::uint64_t name_key;
    std::uint64_t voxel_count;
    std::uint32_t grid_type;
    std::uint32_t grid_class;
    double world_bounds[6];
    std::int32_t index_bounds[6];
    double voxel_size[3];
    std::uint32_t name_size;
    std::uint32_t node_count[4];
    std::uint32_t tile_count[3];
    std::uint16_t codec;
    std::uint16_t blind_data_count;
    std::uint32_t version;
};

static_assert(sizeof(RawGridHeader) == 40);
static_assert(sizeof(RawFileHeader) == sizeof(nanovdb::io::FileHeader));
static_assert(sizeof(RawFileMetadata) == sizeof(nanovdb::io::FileMetaData));
static_assert(sizeof(nanovdb::GridData) == 672);
static_assert(
    offsetof(RawFileHeader, magic) ==
    offsetof(nanovdb::io::FileHeader, magic));
static_assert(
    offsetof(RawFileHeader, version) ==
    offsetof(nanovdb::io::FileHeader, version));
static_assert(
    offsetof(RawFileHeader, grid_count) ==
    offsetof(nanovdb::io::FileHeader, gridCount));
static_assert(
    offsetof(RawFileHeader, codec) ==
    offsetof(nanovdb::io::FileHeader, codec));
static_assert(
    offsetof(RawFileMetadata, grid_size) ==
    offsetof(nanovdb::io::FileMetaData, gridSize));
static_assert(
    offsetof(RawFileMetadata, file_size) ==
    offsetof(nanovdb::io::FileMetaData, fileSize));
static_assert(
    offsetof(RawFileMetadata, grid_type) ==
    offsetof(nanovdb::io::FileMetaData, gridType));
static_assert(
    offsetof(RawFileMetadata, grid_class) ==
    offsetof(nanovdb::io::FileMetaData, gridClass));
static_assert(
    offsetof(RawFileMetadata, name_size) ==
    offsetof(nanovdb::io::FileMetaData, nameSize));
static_assert(
    offsetof(RawFileMetadata, codec) ==
    offsetof(nanovdb::io::FileMetaData, codec));
static_assert(
    offsetof(RawFileMetadata, version) ==
    offsetof(nanovdb::io::FileMetaData, version));
static_assert(offsetof(nanovdb::GridData, mGridClass) == GRID_CLASS_OFFSET);
static_assert(offsetof(nanovdb::GridData, mGridType) == GRID_TYPE_OFFSET);
static_assert(std::is_trivially_copyable_v<RawGridHeader>);
static_assert(std::is_trivially_copyable_v<RawFileHeader>);
static_assert(std::is_trivially_copyable_v<RawFileMetadata>);

#define BF_ASSERT_GRID_TYPE(name, upstream)                                      \
    static_assert(                                                              \
        BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_##name ==                                   \
        static_cast<std::uint32_t>(nanovdb::GridType::upstream))

BF_ASSERT_GRID_TYPE(UNKNOWN, Unknown);
BF_ASSERT_GRID_TYPE(FLOAT, Float);
BF_ASSERT_GRID_TYPE(DOUBLE, Double);
BF_ASSERT_GRID_TYPE(INT16, Int16);
BF_ASSERT_GRID_TYPE(INT32, Int32);
BF_ASSERT_GRID_TYPE(INT64, Int64);
BF_ASSERT_GRID_TYPE(VEC3F, Vec3f);
BF_ASSERT_GRID_TYPE(VEC3D, Vec3d);
BF_ASSERT_GRID_TYPE(MASK, Mask);
BF_ASSERT_GRID_TYPE(HALF, Half);
BF_ASSERT_GRID_TYPE(UINT32, UInt32);
BF_ASSERT_GRID_TYPE(BOOLEAN, Boolean);
BF_ASSERT_GRID_TYPE(RGBA8, RGBA8);
BF_ASSERT_GRID_TYPE(FP4, Fp4);
BF_ASSERT_GRID_TYPE(FP8, Fp8);
BF_ASSERT_GRID_TYPE(FP16, Fp16);
BF_ASSERT_GRID_TYPE(FPN, FpN);
BF_ASSERT_GRID_TYPE(VEC4F, Vec4f);
BF_ASSERT_GRID_TYPE(VEC4D, Vec4d);
BF_ASSERT_GRID_TYPE(INDEX, Index);
BF_ASSERT_GRID_TYPE(ON_INDEX, OnIndex);
BF_ASSERT_GRID_TYPE(POINT_INDEX, PointIndex);
BF_ASSERT_GRID_TYPE(VEC3U8, Vec3u8);
BF_ASSERT_GRID_TYPE(VEC3U16, Vec3u16);
BF_ASSERT_GRID_TYPE(UINT8, UInt8);

#undef BF_ASSERT_GRID_TYPE

#define BF_ASSERT_GRID_CLASS(name, upstream)                                    \
    static_assert(                                                              \
        BF_RENDERING_VOLUMES_NANOVDB_GRID_CLASS_##name ==                                  \
        static_cast<std::uint32_t>(nanovdb::GridClass::upstream))

BF_ASSERT_GRID_CLASS(UNKNOWN, Unknown);
BF_ASSERT_GRID_CLASS(LEVEL_SET, LevelSet);
BF_ASSERT_GRID_CLASS(FOG_VOLUME, FogVolume);
BF_ASSERT_GRID_CLASS(STAGGERED, Staggered);
BF_ASSERT_GRID_CLASS(POINT_INDEX, PointIndex);
BF_ASSERT_GRID_CLASS(POINT_DATA, PointData);
BF_ASSERT_GRID_CLASS(TOPOLOGY, Topology);
BF_ASSERT_GRID_CLASS(VOXEL_VOLUME, VoxelVolume);
BF_ASSERT_GRID_CLASS(INDEX_GRID, IndexGrid);
BF_ASSERT_GRID_CLASS(TENSOR_GRID, TensorGrid);

#undef BF_ASSERT_GRID_CLASS

bool range_fits(std::size_t offset, std::size_t amount, std::size_t size)
{
    return offset <= size && amount <= size - offset;
}

template <typename T>
bool read_pod(const std::uint8_t *data, std::size_t size, std::size_t offset, T *out)
{
    if (!range_fits(offset, sizeof(T), size)) {
        return false;
    }
    std::memcpy(out, data + offset, sizeof(T));
    return true;
}

bool is_compatible_version(std::uint32_t version)
{
    return version >> 21U == NANOVDB_MAJOR_VERSION_NUMBER;
}

bool is_valid_grid_kind(std::uint32_t grid_type, std::uint32_t grid_class)
{
    if (grid_type >= static_cast<std::uint32_t>(nanovdb::GridType::End) ||
        grid_class >= static_cast<std::uint32_t>(nanovdb::GridClass::End)) {
        return false;
    }
    return nanovdb::isValid(
        static_cast<nanovdb::GridType>(grid_type),
        static_cast<nanovdb::GridClass>(grid_class));
}

bool read_grid_kind(
    const std::uint8_t *data,
    std::size_t size,
    std::size_t offset,
    std::uint32_t *out_type,
    std::uint32_t *out_class)
{
    return read_pod(data, size, offset + GRID_TYPE_OFFSET, out_type) &&
        read_pod(data, size, offset + GRID_CLASS_OFFSET, out_class);
}

bool preflight_grid(
    const std::uint8_t *data,
    std::size_t size,
    std::size_t offset,
    const RawFileMetadata *metadata,
    RawGridHeader *out_header)
{
    RawGridHeader header{};
    std::uint32_t grid_type = 0;
    std::uint32_t grid_class = 0;
    if (!read_pod(data, size, offset, &header) ||
        !read_grid_kind(data, size, offset, &grid_type, &grid_class)) {
        return false;
    }
    if (header.magic != NANOVDB_MAGIC_GRID ||
        !is_compatible_version(header.version) ||
        header.grid_count == 0 ||
        header.grid_index >= header.grid_count ||
        header.grid_size < MINIMUM_GRID_SIZE ||
        header.grid_size % NANOVDB_DATA_ALIGNMENT != 0 ||
        header.grid_size > std::numeric_limits<std::size_t>::max() ||
        !range_fits(offset, static_cast<std::size_t>(header.grid_size), size) ||
        !is_valid_grid_kind(grid_type, grid_class)) {
        return false;
    }
    if (metadata != nullptr &&
        (metadata->grid_size != header.grid_size ||
         metadata->file_size != header.grid_size ||
         metadata->grid_type != grid_type ||
         metadata->grid_class != grid_class)) {
        return false;
    }
    *out_header = header;
    return true;
}

std::int32_t preflight_raw(const std::uint8_t *data, std::size_t size)
{
    std::size_t offset = 0;
    std::uint32_t grid_count = 0;
    std::uint32_t grid_index = 0;
    while (offset < size) {
        RawGridHeader header{};
        if (!preflight_grid(data, size, offset, nullptr, &header)) {
            return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
        }
        if (offset == 0) {
            grid_count = header.grid_count;
        }
        if (header.grid_count != grid_count ||
            header.grid_index != grid_index ||
            header.grid_index >= grid_count) {
            return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
        }
        offset += static_cast<std::size_t>(header.grid_size);
        ++grid_index;
    }
    return offset == size && grid_index == grid_count
        ? BF_RENDERING_VOLUMES_NANOVDB_STATUS_OK
        : BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
}

std::int32_t preflight_file(const std::uint8_t *data, std::size_t size)
{
    std::size_t offset = 0;
    while (offset < size) {
        RawFileHeader header{};
        if (!read_pod(data, size, offset, &header) ||
            (header.magic != NANOVDB_MAGIC_FILE &&
             header.magic != NANOVDB_MAGIC_NUMB) ||
            !is_compatible_version(header.version) ||
            header.grid_count == 0) {
            return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
        }
        if (header.codec !=
            static_cast<std::uint16_t>(nanovdb::io::Codec::NONE)) {
            return BF_RENDERING_VOLUMES_NANOVDB_STATUS_UNSUPPORTED_COMPRESSION;
        }
        offset += sizeof(header);

        std::vector<RawFileMetadata> metadata;
        metadata.reserve(header.grid_count);
        for (std::uint16_t index = 0; index < header.grid_count; ++index) {
            RawFileMetadata grid{};
            if (!read_pod(data, size, offset, &grid)) {
                return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
            }
            offset += sizeof(grid);
            if (!is_compatible_version(grid.version) ||
                grid.codec != header.codec ||
                grid.grid_size < MINIMUM_GRID_SIZE ||
                grid.file_size != grid.grid_size ||
                grid.name_size == 0 ||
                !is_valid_grid_kind(grid.grid_type, grid.grid_class) ||
                !range_fits(offset, grid.name_size, size) ||
                data[offset + grid.name_size - 1] != '\0') {
                return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
            }
            offset += grid.name_size;
            metadata.push_back(grid);
        }

        for (const RawFileMetadata &grid : metadata) {
            RawGridHeader grid_header{};
            if (!preflight_grid(data, size, offset, &grid, &grid_header)) {
                return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
            }
            offset += static_cast<std::size_t>(grid.file_size);
        }
    }
    return offset == size ? BF_RENDERING_VOLUMES_NANOVDB_STATUS_OK
                          : BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
}

std::int32_t preflight(const std::uint8_t *data, std::size_t size)
{
    std::uint64_t magic = 0;
    if (!read_pod(data, size, 0, &magic)) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
    }
    if (magic == NANOVDB_MAGIC_GRID) {
        return preflight_raw(data, size);
    }
    if (magic == NANOVDB_MAGIC_FILE || magic == NANOVDB_MAGIC_NUMB) {
        return preflight_file(data, size);
    }
    return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
}

const nanovdb::GridData *find_grid(
    const BFRenderingVolumesNanoVdb *handle,
    std::uint32_t grid_index)
{
    if (handle == nullptr || grid_index >= handle->grids.size()) {
        return nullptr;
    }
    return handle->grids[grid_index];
}

BFRenderingVolumesNanoVdbVec3d to_native_vec(const nanovdb::Vec3d &value)
{
    return {value[0], value[1], value[2]};
}

nanovdb::Vec3d to_nanovdb_vec(BFRenderingVolumesNanoVdbVec3d value)
{
    return {value.x, value.y, value.z};
}

BFRenderingVolumesNanoVdbCoord to_native_coord(const nanovdb::Coord &value)
{
    return {value[0], value[1], value[2]};
}

template <typename BuildT>
std::int32_t float_voxel(
    const nanovdb::GridData *grid_data,
    BFRenderingVolumesNanoVdbCoord coordinate,
    float *out_value,
    std::uint8_t *out_active)
{
    const auto *grid =
        reinterpret_cast<const nanovdb::NanoGrid<BuildT> *>(grid_data);
    auto accessor = grid->getAccessor();
    const nanovdb::Coord index(coordinate.x, coordinate.y, coordinate.z);
    *out_value = accessor.getValue(index);
    *out_active = accessor.isActive(index) ? 1 : 0;
    return BF_RENDERING_VOLUMES_NANOVDB_STATUS_OK;
}

template <typename BuildT>
std::int32_t sample_float_world(
    const nanovdb::GridData *grid_data,
    BFRenderingVolumesNanoVdbVec3d position,
    float *out_value)
{
    const auto *grid =
        reinterpret_cast<const nanovdb::NanoGrid<BuildT> *>(grid_data);
    auto accessor = grid->getAccessor();
    const nanovdb::math::SampleFromVoxels<decltype(accessor), 1, false> sampler(
        accessor);
    *out_value = sampler(grid->worldToIndex(to_nanovdb_vec(position)));
    return BF_RENDERING_VOLUMES_NANOVDB_STATUS_OK;
}

template <typename Operation, typename... Arguments>
std::int32_t dispatch_float_grid(
    const nanovdb::GridData *grid,
    Operation operation,
    Arguments &&...arguments)
{
    switch (grid->mGridType) {
    case nanovdb::GridType::Float:
        return operation.template operator()<float>(
            grid,
            std::forward<Arguments>(arguments)...);
    case nanovdb::GridType::Fp4:
        return operation.template operator()<nanovdb::Fp4>(
            grid,
            std::forward<Arguments>(arguments)...);
    case nanovdb::GridType::Fp8:
        return operation.template operator()<nanovdb::Fp8>(
            grid,
            std::forward<Arguments>(arguments)...);
    case nanovdb::GridType::Fp16:
        return operation.template operator()<nanovdb::Fp16>(
            grid,
            std::forward<Arguments>(arguments)...);
    case nanovdb::GridType::FpN:
        return operation.template operator()<nanovdb::FpN>(
            grid,
            std::forward<Arguments>(arguments)...);
    default:
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_TYPE_MISMATCH;
    }
}

struct FloatVoxelOperation {
    template <typename BuildT>
    std::int32_t operator()(
        const nanovdb::GridData *grid,
        BFRenderingVolumesNanoVdbCoord coordinate,
        float *out_value,
        std::uint8_t *out_active) const
    {
        return float_voxel<BuildT>(grid, coordinate, out_value, out_active);
    }
};

struct SampleFloatOperation {
    template <typename BuildT>
    std::int32_t operator()(
        const nanovdb::GridData *grid,
        BFRenderingVolumesNanoVdbVec3d position,
        float *out_value) const
    {
        return sample_float_world<BuildT>(grid, position, out_value);
    }
};

} // namespace

extern "C" BFRenderingVolumesNanoVdbVersion bf_rendering_volumes_openvdb_version(void)
{
    return {
        BF_OPENVDB_VERSION_MAJOR,
        BF_OPENVDB_VERSION_MINOR,
        BF_OPENVDB_VERSION_PATCH,
    };
}

extern "C" BFRenderingVolumesNanoVdbVersion bf_rendering_volumes_nanovdb_version(void)
{
    return {
        NANOVDB_MAJOR_VERSION_NUMBER,
        NANOVDB_MINOR_VERSION_NUMBER,
        NANOVDB_PATCH_VERSION_NUMBER,
    };
}

extern "C" std::int32_t bf_rendering_volumes_nanovdb_load(
    const std::uint8_t *data,
    std::size_t size,
    BFRenderingVolumesNanoVdb **out_handle)
{
    if (out_handle == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_NULL_POINTER;
    }
    *out_handle = nullptr;
    if (data == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_NULL_POINTER;
    }
    if (size == 0) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ARGUMENT;
    }

    try {
        const std::int32_t preflight_status = preflight(data, size);
        if (preflight_status != BF_RENDERING_VOLUMES_NANOVDB_STATUS_OK) {
            return preflight_status;
        }

        const std::string bytes(reinterpret_cast<const char *>(data), size);
        std::istringstream stream(bytes, std::ios::in | std::ios::binary);
        auto handles = nanovdb::io::readGrids(stream);
        if (handles.empty()) {
            return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
        }
        for (const auto &handle : handles) {
            if (!nanovdb::tools::validateGrids(
                    handle,
                    nanovdb::CheckMode::Half,
                    false)) {
                return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
            }
        }

        auto owner = std::make_unique<BFRenderingVolumesNanoVdb>();
        owner->handles = std::move(handles);
        std::size_t grid_count = 0;
        for (const auto &handle : owner->handles) {
            grid_count += handle.gridCount();
        }
        if (grid_count == 0 ||
            grid_count > std::numeric_limits<std::uint32_t>::max()) {
            return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
        }
        owner->grids.reserve(grid_count);
        for (const auto &handle : owner->handles) {
            for (std::uint32_t index = 0; index < handle.gridCount(); ++index) {
                owner->grids.push_back(handle.gridData(index));
            }
        }
        *out_handle = owner.release();
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_OK;
    } catch (const std::bad_alloc &) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_OUT_OF_MEMORY;
    } catch (const std::exception &) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET;
    } catch (...) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_NATIVE_FAILURE;
    }
}

extern "C" void bf_rendering_volumes_nanovdb_destroy(BFRenderingVolumesNanoVdb *handle)
{
    delete handle;
}

extern "C" std::uint32_t bf_rendering_volumes_nanovdb_grid_count(
    const BFRenderingVolumesNanoVdb *handle)
{
    return handle == nullptr ? 0 : static_cast<std::uint32_t>(handle->grids.size());
}

extern "C" std::int32_t bf_rendering_volumes_nanovdb_grid_name(
    const BFRenderingVolumesNanoVdb *handle,
    std::uint32_t grid_index,
    const char **out_name,
    std::size_t *out_length)
{
    if (handle == nullptr || out_name == nullptr || out_length == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_NULL_POINTER;
    }
    const nanovdb::GridData *grid = find_grid(handle, grid_index);
    if (grid == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INDEX_OUT_OF_RANGE;
    }
    *out_name = grid->gridName();
    *out_length = std::strlen(*out_name);
    return BF_RENDERING_VOLUMES_NANOVDB_STATUS_OK;
}

extern "C" std::int32_t bf_rendering_volumes_nanovdb_grid_info(
    const BFRenderingVolumesNanoVdb *handle,
    std::uint32_t grid_index,
    BFRenderingVolumesNanoVdbGridInfo *out_info)
{
    if (handle == nullptr || out_info == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_NULL_POINTER;
    }
    const nanovdb::GridData *grid = find_grid(handle, grid_index);
    if (grid == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INDEX_OUT_OF_RANGE;
    }

    const nanovdb::GridMetaData metadata(grid);
    const nanovdb::CoordBBox &index_bounds = metadata.indexBBox();
    const nanovdb::Vec3dBBox &world_bounds = metadata.worldBBox();
    *out_info = {
        static_cast<std::uint32_t>(metadata.gridType()),
        static_cast<std::uint32_t>(metadata.gridClass()),
        metadata.gridSize(),
        metadata.activeVoxelCount(),
        to_native_coord(index_bounds.min()),
        to_native_coord(index_bounds.max()),
        to_native_vec(world_bounds.min()),
        to_native_vec(world_bounds.max()),
        to_native_vec(metadata.voxelSize()),
        metadata.isEmpty() ? std::uint8_t{1} : std::uint8_t{0},
    };
    return BF_RENDERING_VOLUMES_NANOVDB_STATUS_OK;
}

extern "C" std::int32_t bf_rendering_volumes_nanovdb_index_to_world(
    const BFRenderingVolumesNanoVdb *handle,
    std::uint32_t grid_index,
    BFRenderingVolumesNanoVdbVec3d position,
    BFRenderingVolumesNanoVdbVec3d *out_position)
{
    if (handle == nullptr || out_position == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_NULL_POINTER;
    }
    const nanovdb::GridData *grid = find_grid(handle, grid_index);
    if (grid == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INDEX_OUT_OF_RANGE;
    }
    *out_position = to_native_vec(grid->applyMap(to_nanovdb_vec(position)));
    return BF_RENDERING_VOLUMES_NANOVDB_STATUS_OK;
}

extern "C" std::int32_t bf_rendering_volumes_nanovdb_world_to_index(
    const BFRenderingVolumesNanoVdb *handle,
    std::uint32_t grid_index,
    BFRenderingVolumesNanoVdbVec3d position,
    BFRenderingVolumesNanoVdbVec3d *out_position)
{
    if (handle == nullptr || out_position == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_NULL_POINTER;
    }
    const nanovdb::GridData *grid = find_grid(handle, grid_index);
    if (grid == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INDEX_OUT_OF_RANGE;
    }
    *out_position =
        to_native_vec(grid->applyInverseMap(to_nanovdb_vec(position)));
    return BF_RENDERING_VOLUMES_NANOVDB_STATUS_OK;
}

extern "C" std::int32_t bf_rendering_volumes_nanovdb_float_voxel(
    const BFRenderingVolumesNanoVdb *handle,
    std::uint32_t grid_index,
    BFRenderingVolumesNanoVdbCoord coordinate,
    float *out_value,
    std::uint8_t *out_active)
{
    if (handle == nullptr || out_value == nullptr || out_active == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_NULL_POINTER;
    }
    const nanovdb::GridData *grid = find_grid(handle, grid_index);
    if (grid == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INDEX_OUT_OF_RANGE;
    }
    return dispatch_float_grid(
        grid,
        FloatVoxelOperation{},
        coordinate,
        out_value,
        out_active);
}

extern "C" std::int32_t bf_rendering_volumes_nanovdb_sample_float_world(
    const BFRenderingVolumesNanoVdb *handle,
    std::uint32_t grid_index,
    BFRenderingVolumesNanoVdbVec3d position,
    float *out_value)
{
    if (handle == nullptr || out_value == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_NULL_POINTER;
    }
    const nanovdb::GridData *grid = find_grid(handle, grid_index);
    if (grid == nullptr) {
        return BF_RENDERING_VOLUMES_NANOVDB_STATUS_INDEX_OUT_OF_RANGE;
    }
    return dispatch_float_grid(
        grid,
        SampleFloatOperation{},
        position,
        out_value);
}
