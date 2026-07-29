#ifndef BLACKFLOWER_RENDER_NANOVDB_WRAPPER_H
#define BLACKFLOWER_RENDER_NANOVDB_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BF_RENDER_NANOVDB_STATUS_OK 0
#define BF_RENDER_NANOVDB_STATUS_NULL_POINTER 1
#define BF_RENDER_NANOVDB_STATUS_INVALID_ARGUMENT 2
#define BF_RENDER_NANOVDB_STATUS_INVALID_ASSET 3
#define BF_RENDER_NANOVDB_STATUS_UNSUPPORTED_COMPRESSION 4
#define BF_RENDER_NANOVDB_STATUS_OUT_OF_MEMORY 5
#define BF_RENDER_NANOVDB_STATUS_INDEX_OUT_OF_RANGE 6
#define BF_RENDER_NANOVDB_STATUS_TYPE_MISMATCH 7
#define BF_RENDER_NANOVDB_STATUS_NATIVE_FAILURE 8

#define BF_RENDER_NANOVDB_GRID_TYPE_UNKNOWN 0
#define BF_RENDER_NANOVDB_GRID_TYPE_FLOAT 1
#define BF_RENDER_NANOVDB_GRID_TYPE_DOUBLE 2
#define BF_RENDER_NANOVDB_GRID_TYPE_INT16 3
#define BF_RENDER_NANOVDB_GRID_TYPE_INT32 4
#define BF_RENDER_NANOVDB_GRID_TYPE_INT64 5
#define BF_RENDER_NANOVDB_GRID_TYPE_VEC3F 6
#define BF_RENDER_NANOVDB_GRID_TYPE_VEC3D 7
#define BF_RENDER_NANOVDB_GRID_TYPE_MASK 8
#define BF_RENDER_NANOVDB_GRID_TYPE_HALF 9
#define BF_RENDER_NANOVDB_GRID_TYPE_UINT32 10
#define BF_RENDER_NANOVDB_GRID_TYPE_BOOLEAN 11
#define BF_RENDER_NANOVDB_GRID_TYPE_RGBA8 12
#define BF_RENDER_NANOVDB_GRID_TYPE_FP4 13
#define BF_RENDER_NANOVDB_GRID_TYPE_FP8 14
#define BF_RENDER_NANOVDB_GRID_TYPE_FP16 15
#define BF_RENDER_NANOVDB_GRID_TYPE_FPN 16
#define BF_RENDER_NANOVDB_GRID_TYPE_VEC4F 17
#define BF_RENDER_NANOVDB_GRID_TYPE_VEC4D 18
#define BF_RENDER_NANOVDB_GRID_TYPE_INDEX 19
#define BF_RENDER_NANOVDB_GRID_TYPE_ON_INDEX 20
#define BF_RENDER_NANOVDB_GRID_TYPE_POINT_INDEX 23
#define BF_RENDER_NANOVDB_GRID_TYPE_VEC3U8 24
#define BF_RENDER_NANOVDB_GRID_TYPE_VEC3U16 25
#define BF_RENDER_NANOVDB_GRID_TYPE_UINT8 26

#define BF_RENDER_NANOVDB_GRID_CLASS_UNKNOWN 0
#define BF_RENDER_NANOVDB_GRID_CLASS_LEVEL_SET 1
#define BF_RENDER_NANOVDB_GRID_CLASS_FOG_VOLUME 2
#define BF_RENDER_NANOVDB_GRID_CLASS_STAGGERED 3
#define BF_RENDER_NANOVDB_GRID_CLASS_POINT_INDEX 4
#define BF_RENDER_NANOVDB_GRID_CLASS_POINT_DATA 5
#define BF_RENDER_NANOVDB_GRID_CLASS_TOPOLOGY 6
#define BF_RENDER_NANOVDB_GRID_CLASS_VOXEL_VOLUME 7
#define BF_RENDER_NANOVDB_GRID_CLASS_INDEX_GRID 8
#define BF_RENDER_NANOVDB_GRID_CLASS_TENSOR_GRID 9

typedef struct BFRenderNanoVdb BFRenderNanoVdb;

typedef struct BFRenderNanoVdbVersion {
    uint32_t major;
    uint32_t minor;
    uint32_t patch;
} BFRenderNanoVdbVersion;

typedef struct BFRenderNanoVdbCoord {
    int32_t x;
    int32_t y;
    int32_t z;
} BFRenderNanoVdbCoord;

typedef struct BFRenderNanoVdbVec3d {
    double x;
    double y;
    double z;
} BFRenderNanoVdbVec3d;

typedef struct BFRenderNanoVdbGridInfo {
    uint32_t grid_type;
    uint32_t grid_class;
    uint64_t byte_size;
    uint64_t active_voxel_count;
    BFRenderNanoVdbCoord index_min;
    BFRenderNanoVdbCoord index_max;
    BFRenderNanoVdbVec3d world_min;
    BFRenderNanoVdbVec3d world_max;
    BFRenderNanoVdbVec3d voxel_size;
    uint8_t is_empty;
} BFRenderNanoVdbGridInfo;

BFRenderNanoVdbVersion bf_render_openvdb_version(void);
BFRenderNanoVdbVersion bf_render_nanovdb_version(void);

int32_t bf_render_nanovdb_load(
    const uint8_t *data,
    size_t size,
    BFRenderNanoVdb **out_handle);
void bf_render_nanovdb_destroy(BFRenderNanoVdb *handle);
uint32_t bf_render_nanovdb_grid_count(const BFRenderNanoVdb *handle);

int32_t bf_render_nanovdb_grid_name(
    const BFRenderNanoVdb *handle,
    uint32_t grid_index,
    const char **out_name,
    size_t *out_length);
int32_t bf_render_nanovdb_grid_info(
    const BFRenderNanoVdb *handle,
    uint32_t grid_index,
    BFRenderNanoVdbGridInfo *out_info);
int32_t bf_render_nanovdb_index_to_world(
    const BFRenderNanoVdb *handle,
    uint32_t grid_index,
    BFRenderNanoVdbVec3d position,
    BFRenderNanoVdbVec3d *out_position);
int32_t bf_render_nanovdb_world_to_index(
    const BFRenderNanoVdb *handle,
    uint32_t grid_index,
    BFRenderNanoVdbVec3d position,
    BFRenderNanoVdbVec3d *out_position);
int32_t bf_render_nanovdb_float_voxel(
    const BFRenderNanoVdb *handle,
    uint32_t grid_index,
    BFRenderNanoVdbCoord coordinate,
    float *out_value,
    uint8_t *out_active);
int32_t bf_render_nanovdb_sample_float_world(
    const BFRenderNanoVdb *handle,
    uint32_t grid_index,
    BFRenderNanoVdbVec3d position,
    float *out_value);

#ifdef __cplusplus
}
#endif

#endif
