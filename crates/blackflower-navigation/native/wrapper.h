#ifndef BLACKFLOWER_NAVIGATION_WRAPPER_H
#define BLACKFLOWER_NAVIGATION_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BF_NAVIGATION_STATUS_OK 0
#define BF_NAVIGATION_STATUS_NULL_POINTER 1
#define BF_NAVIGATION_STATUS_INVALID_ARGUMENT 2
#define BF_NAVIGATION_STATUS_OUT_OF_MEMORY 3
#define BF_NAVIGATION_STATUS_INVALID_NAVMESH_DATA 4
#define BF_NAVIGATION_STATUS_INITIALIZATION_FAILED 5
#define BF_NAVIGATION_STATUS_TILE_ALREADY_OCCUPIED 6
#define BF_NAVIGATION_STATUS_QUERY_FAILED 7

#define BF_NAVIGATION_DETAIL_BUFFER_TOO_SMALL (1u << 0)
#define BF_NAVIGATION_DETAIL_OUT_OF_NODES (1u << 1)
#define BF_NAVIGATION_DETAIL_PARTIAL_RESULT (1u << 2)

#define BF_NAVIGATION_STRAIGHT_PATH_START 0x01
#define BF_NAVIGATION_STRAIGHT_PATH_END 0x02
#define BF_NAVIGATION_STRAIGHT_PATH_OFF_MESH_CONNECTION 0x04

#define BF_NAVIGATION_MAX_AREAS 64

typedef struct BFNavigationNavMesh BFNavigationNavMesh;
typedef struct BFNavigationQuery BFNavigationQuery;

typedef unsigned int BFNavigationPolyRef;
typedef unsigned int BFNavigationTileRef;

typedef struct BFNavigationVec3 {
    float x;
    float y;
    float z;
} BFNavigationVec3;

typedef struct BFNavigationVersion {
    uint32_t major;
    uint32_t minor;
    uint32_t patch;
} BFNavigationVersion;

typedef struct BFNavigationNavMeshParams {
    BFNavigationVec3 origin;
    float tile_width;
    float tile_height;
    uint32_t max_tiles;
    uint32_t max_polygons_per_tile;
} BFNavigationNavMeshParams;

typedef struct BFNavigationFilter {
    uint16_t include_flags;
    uint16_t exclude_flags;
    float area_costs[BF_NAVIGATION_MAX_AREAS];
} BFNavigationFilter;

typedef struct BFNavigationNearestPoint {
    BFNavigationPolyRef polygon;
    BFNavigationVec3 position;
    uint8_t is_over_polygon;
} BFNavigationNearestPoint;

typedef struct BFNavigationRaycastResult {
    float fraction;
    BFNavigationVec3 normal;
    int32_t edge_index;
    float path_cost;
} BFNavigationRaycastResult;

BFNavigationVersion bf_navigation_recast_version(void);
uint32_t bf_navigation_detour_navmesh_version(void);

int32_t bf_navigation_navmesh_create_single_tile(
    const uint8_t *data,
    size_t data_size,
    BFNavigationNavMesh **out_navmesh);
int32_t bf_navigation_navmesh_create_tiled(
    const BFNavigationNavMeshParams *params,
    BFNavigationNavMesh **out_navmesh);
void bf_navigation_navmesh_destroy(BFNavigationNavMesh *navmesh);
int32_t bf_navigation_navmesh_add_tile(
    BFNavigationNavMesh *navmesh,
    const uint8_t *data,
    size_t data_size,
    BFNavigationTileRef desired_reference,
    BFNavigationTileRef *out_reference);
int32_t bf_navigation_navmesh_remove_tile(
    BFNavigationNavMesh *navmesh,
    BFNavigationTileRef reference);
int32_t bf_navigation_navmesh_replace_tile(
    BFNavigationNavMesh *navmesh,
    BFNavigationTileRef reference,
    const uint8_t *data,
    size_t data_size,
    BFNavigationTileRef *out_reference);

int32_t bf_navigation_query_create(
    const BFNavigationNavMesh *navmesh,
    uint32_t max_nodes,
    BFNavigationQuery **out_query);
void bf_navigation_query_destroy(BFNavigationQuery *query);

int32_t bf_navigation_query_find_nearest_point(
    const BFNavigationQuery *query,
    BFNavigationVec3 center,
    BFNavigationVec3 half_extents,
    const BFNavigationFilter *filter,
    BFNavigationNearestPoint *out_nearest);
int32_t bf_navigation_query_closest_point_on_polygon(
    const BFNavigationQuery *query,
    BFNavigationPolyRef polygon,
    BFNavigationVec3 position,
    BFNavigationNearestPoint *out_closest);
int32_t bf_navigation_query_find_path(
    const BFNavigationQuery *query,
    BFNavigationPolyRef start_polygon,
    BFNavigationPolyRef end_polygon,
    BFNavigationVec3 start,
    BFNavigationVec3 end,
    const BFNavigationFilter *filter,
    BFNavigationPolyRef *out_path,
    uint32_t path_capacity,
    uint32_t *out_path_count,
    uint32_t *out_details);
int32_t bf_navigation_query_find_straight_path(
    const BFNavigationQuery *query,
    BFNavigationVec3 start,
    BFNavigationVec3 end,
    const BFNavigationPolyRef *path,
    uint32_t path_count,
    BFNavigationVec3 *out_points,
    uint8_t *out_point_flags,
    BFNavigationPolyRef *out_point_polygons,
    uint32_t point_capacity,
    uint32_t *out_point_count,
    uint32_t *out_details);
int32_t bf_navigation_query_raycast(
    const BFNavigationQuery *query,
    BFNavigationPolyRef start_polygon,
    BFNavigationVec3 start,
    BFNavigationVec3 end,
    const BFNavigationFilter *filter,
    BFNavigationPolyRef *out_visited,
    uint32_t visited_capacity,
    uint32_t *out_visited_count,
    uint32_t *out_details,
    BFNavigationRaycastResult *out_result);

#ifdef __cplusplus
}
#endif

#endif
