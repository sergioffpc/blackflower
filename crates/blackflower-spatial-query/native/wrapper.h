#ifndef BLACKFLOWER_SPATIAL_QUERY_WRAPPER_H
#define BLACKFLOWER_SPATIAL_QUERY_WRAPPER_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BF_SPATIAL_QUERY_STATUS_OK 0
#define BF_SPATIAL_QUERY_STATUS_NULL_POINTER 1
#define BF_SPATIAL_QUERY_STATUS_INVALID_ARGUMENT 2
#define BF_SPATIAL_QUERY_STATUS_OUT_OF_MEMORY 3
#define BF_SPATIAL_QUERY_STATUS_NATIVE_FAILURE 4
#define BF_SPATIAL_QUERY_STATUS_SCENE_COMMITTED 5

#define BF_SPATIAL_QUERY_INVALID_ID UINT32_MAX

typedef struct BFSpatialQueryDevice BFSpatialQueryDevice;
typedef struct BFSpatialQueryScene BFSpatialQueryScene;

typedef struct BFSpatialQueryVersion {
    uint32_t major;
    uint32_t minor;
    uint32_t patch;
} BFSpatialQueryVersion;

typedef struct BFSpatialQueryVec3 {
    float x;
    float y;
    float z;
} BFSpatialQueryVec3;

typedef struct BFSpatialQueryTriangle {
    BFSpatialQueryVec3 vertices[3];
} BFSpatialQueryTriangle;

typedef struct BFSpatialQuerySurfaceHit {
    float distance;
    float fraction;
    BFSpatialQueryVec3 geometric_normal;
    float barycentric_u;
    float barycentric_v;
    uint32_t geometry_id;
    uint32_t primitive_id;
    uint32_t instance_id;
} BFSpatialQuerySurfaceHit;

BFSpatialQueryVersion bf_spatial_query_embree_version(void);

int32_t bf_spatial_query_device_create(BFSpatialQueryDevice **out_device);
void bf_spatial_query_device_destroy(BFSpatialQueryDevice *device);

int32_t bf_spatial_query_scene_create(
    BFSpatialQueryDevice *device,
    BFSpatialQueryScene **out_scene);
void bf_spatial_query_scene_destroy(BFSpatialQueryScene *scene);

int32_t bf_spatial_query_scene_add_triangles(
    BFSpatialQueryScene *scene,
    const BFSpatialQueryTriangle *triangles,
    uint32_t triangle_count,
    uint32_t *out_geometry_id);

int32_t bf_spatial_query_scene_commit(BFSpatialQueryScene *scene);

int32_t bf_spatial_query_scene_intersect_segment(
    const BFSpatialQueryScene *scene,
    BFSpatialQueryVec3 start,
    BFSpatialQueryVec3 end,
    uint32_t max_hits,
    BFSpatialQuerySurfaceHit *out_hits,
    uint32_t *out_hit_count);

#ifdef __cplusplus
}
#endif

#endif
