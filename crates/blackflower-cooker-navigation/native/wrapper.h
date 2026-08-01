#ifndef BLACKFLOWER_NAVIGATION_COOKER_H
#define BLACKFLOWER_NAVIGATION_COOKER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BF_NAVIGATION_COOK_OK 0
#define BF_NAVIGATION_COOK_INVALID_INPUT 1
#define BF_NAVIGATION_COOK_OUT_OF_MEMORY 2
#define BF_NAVIGATION_COOK_RECAST_FAILED 3
#define BF_NAVIGATION_COOK_DETOUR_FAILED 4

typedef struct BFNavigationCookSettings {
    float cell_size;
    float cell_height;
    uint32_t tile_size;
    uint32_t region_min_area;
    uint32_t region_merge_area;
    float max_edge_length;
    float max_simplification_error;
    uint32_t max_vertices_per_polygon;
    float detail_sample_distance;
    float detail_sample_max_error;
    float agent_height;
    float agent_radius;
    float agent_max_climb;
    float agent_max_slope_degrees;
} BFNavigationCookSettings;

typedef struct BFNavigationCookInput {
    const float *vertices;
    uint32_t vertex_count;
    const int32_t *indices;
    const uint8_t *triangle_areas;
    uint32_t triangle_count;
    const uint8_t *area_remap;
    const uint8_t *area_traversable;
    const float *off_mesh_vertices;
    const float *off_mesh_radii;
    const uint8_t *off_mesh_directions;
    const uint8_t *off_mesh_areas;
    const uint16_t *off_mesh_flags;
    const uint32_t *off_mesh_user_ids;
    uint32_t off_mesh_count;
} BFNavigationCookInput;

typedef struct BFNavigationCookedTile {
    int32_t x;
    int32_t y;
    int32_t layer;
    uint8_t *data;
    uint32_t data_size;
} BFNavigationCookedTile;

typedef struct BFNavigationCookOutput {
    float origin[3];
    float tile_width;
    float tile_height;
    uint32_t max_tiles;
    uint32_t max_polygons_per_tile;
    BFNavigationCookedTile *tiles;
    uint32_t tile_count;
} BFNavigationCookOutput;

int32_t bf_navigation_cooker_build(
    const BFNavigationCookSettings *settings,
    const BFNavigationCookInput *input,
    BFNavigationCookOutput *output,
    char *error,
    size_t error_capacity);
void bf_navigation_cooker_free(BFNavigationCookOutput *output);

#ifdef __cplusplus
}
#endif

#endif
