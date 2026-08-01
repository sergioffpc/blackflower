#include "wrapper.h"

#include <DetourAlloc.h>
#include <DetourNavMesh.h>
#include <DetourNavMeshBuilder.h>
#include <Recast.h>

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <limits>
#include <new>
#include <vector>

static_assert(sizeof(int) == sizeof(int32_t), "Recast indices require 32-bit int");

namespace {

struct TileBuild {
    int32_t x;
    int32_t y;
    int32_t layer;
    std::vector<uint8_t> data;
    uint32_t polygons;
};

void set_error(char *error, size_t capacity, const char *message) {
    if (error == nullptr || capacity == 0) {
        return;
    }
    std::snprintf(error, capacity, "%s", message);
}

bool finite_positive(float value) {
    return std::isfinite(value) && value > 0.0F;
}

bool finite_non_negative(float value) {
    return std::isfinite(value) && value >= 0.0F;
}

bool valid_settings(const BFNavigationCookSettings &settings) {
    if (!finite_positive(settings.cell_size)
        || !finite_positive(settings.cell_height)) {
        return false;
    }
    const double maximum = static_cast<double>(std::numeric_limits<int>::max());
    const double walkable_height =
        std::ceil(static_cast<double>(settings.agent_height) / settings.cell_height);
    const double walkable_radius =
        std::ceil(static_cast<double>(settings.agent_radius) / settings.cell_size);
    const double walkable_climb =
        std::floor(static_cast<double>(settings.agent_max_climb) / settings.cell_height);
    const double edge_length =
        static_cast<double>(settings.max_edge_length) / settings.cell_size;
    const double raster_width =
        static_cast<double>(settings.tile_size) + (walkable_radius + 3.0) * 2.0;
    return finite_positive(settings.cell_size)
        && finite_positive(settings.cell_height)
        && settings.tile_size > 0
        && settings.tile_size <= static_cast<uint32_t>(std::numeric_limits<int>::max())
        && settings.region_min_area > 0
        && settings.region_min_area <= 46340
        && settings.region_merge_area > 0
        && settings.region_merge_area <= 46340
        && finite_positive(settings.max_edge_length)
        && finite_positive(settings.max_simplification_error)
        && settings.max_vertices_per_polygon >= 3
        && settings.max_vertices_per_polygon <= DT_VERTS_PER_POLYGON
        && finite_non_negative(settings.detail_sample_distance)
        && finite_non_negative(settings.detail_sample_max_error)
        && finite_positive(settings.agent_height)
        && finite_positive(settings.agent_radius)
        && finite_non_negative(settings.agent_max_climb)
        && std::isfinite(settings.agent_max_slope_degrees)
        && settings.agent_max_slope_degrees >= 0.0F
        && settings.agent_max_slope_degrees < 90.0F
        && walkable_height <= maximum
        && walkable_radius <= maximum
        && walkable_climb <= maximum
        && edge_length <= maximum
        && raster_width <= maximum;
}

bool valid_input(const BFNavigationCookInput &input) {
    if (input.vertices == nullptr || input.vertex_count < 3 || input.indices == nullptr
        || input.triangle_areas == nullptr || input.triangle_count == 0
        || input.area_remap == nullptr || input.area_traversable == nullptr
        || input.vertex_count > static_cast<uint32_t>(std::numeric_limits<int>::max())
        || input.triangle_count > static_cast<uint32_t>(std::numeric_limits<int>::max())
        || input.off_mesh_count
            > static_cast<uint32_t>(std::numeric_limits<int>::max())) {
        return false;
    }
    for (uint32_t triangle = 0; triangle < input.triangle_count; ++triangle) {
        for (uint32_t corner = 0; corner < 3; ++corner) {
            const size_t offset =
                static_cast<size_t>(triangle) * 3 + static_cast<size_t>(corner);
            const int32_t index = input.indices[offset];
            if (index < 0 || static_cast<uint32_t>(index) >= input.vertex_count) {
                return false;
            }
        }
        if (input.triangle_areas[triangle] > 63) {
            return false;
        }
    }
    const size_t coordinate_count = static_cast<size_t>(input.vertex_count) * 3;
    for (size_t vertex = 0; vertex < coordinate_count; ++vertex) {
        if (!std::isfinite(input.vertices[vertex])) {
            return false;
        }
    }
    if (input.off_mesh_count == 0) {
        return true;
    }
    return input.off_mesh_vertices != nullptr && input.off_mesh_radii != nullptr
        && input.off_mesh_directions != nullptr && input.off_mesh_areas != nullptr
        && input.off_mesh_flags != nullptr && input.off_mesh_user_ids != nullptr;
}

uint32_t next_power_of_two(uint32_t value) {
    if (value <= 1) {
        return 1;
    }
    --value;
    value |= value >> 1;
    value |= value >> 2;
    value |= value >> 4;
    value |= value >> 8;
    value |= value >> 16;
    return value + 1;
}

uint32_t integer_log2(uint32_t value) {
    uint32_t result = 0;
    while (value > 1) {
        value >>= 1;
        ++result;
    }
    return result;
}

void free_intermediates(
    rcHeightfield *solid,
    rcCompactHeightfield *compact,
    rcContourSet *contours,
    rcPolyMesh *poly_mesh,
    rcPolyMeshDetail *detail_mesh) {
    rcFreeHeightField(solid);
    rcFreeCompactHeightfield(compact);
    rcFreeContourSet(contours);
    rcFreePolyMesh(poly_mesh);
    rcFreePolyMeshDetail(detail_mesh);
}

int32_t build_tile(
    const BFNavigationCookSettings &settings,
    const BFNavigationCookInput &input,
    const float *tile_min,
    const float *tile_max,
    int32_t tile_x,
    int32_t tile_y,
    TileBuild &result,
    char *error,
    size_t error_capacity) {
    rcContext context;
    rcConfig config {};
    config.cs = settings.cell_size;
    config.ch = settings.cell_height;
    config.walkableSlopeAngle = settings.agent_max_slope_degrees;
    config.walkableHeight = static_cast<int>(std::ceil(settings.agent_height / config.ch));
    config.walkableClimb = static_cast<int>(std::floor(settings.agent_max_climb / config.ch));
    config.walkableRadius = static_cast<int>(std::ceil(settings.agent_radius / config.cs));
    config.maxEdgeLen = static_cast<int>(settings.max_edge_length / config.cs);
    config.maxSimplificationError = settings.max_simplification_error;
    config.minRegionArea = static_cast<int>(
        settings.region_min_area * settings.region_min_area);
    config.mergeRegionArea = static_cast<int>(
        settings.region_merge_area * settings.region_merge_area);
    config.maxVertsPerPoly = static_cast<int>(settings.max_vertices_per_polygon);
    config.tileSize = static_cast<int>(settings.tile_size);
    config.borderSize = config.walkableRadius + 3;
    config.width = config.tileSize + config.borderSize * 2;
    config.height = config.tileSize + config.borderSize * 2;
    config.detailSampleDist = settings.detail_sample_distance < 0.9F
        ? 0.0F
        : config.cs * settings.detail_sample_distance;
    config.detailSampleMaxError =
        config.ch * settings.detail_sample_max_error;
    rcVcopy(config.bmin, tile_min);
    rcVcopy(config.bmax, tile_max);
    config.bmin[0] -= static_cast<float>(config.borderSize) * config.cs;
    config.bmin[2] -= static_cast<float>(config.borderSize) * config.cs;
    config.bmax[0] += static_cast<float>(config.borderSize) * config.cs;
    config.bmax[2] += static_cast<float>(config.borderSize) * config.cs;

    rcHeightfield *solid = rcAllocHeightfield();
    rcCompactHeightfield *compact = nullptr;
    rcContourSet *contours = nullptr;
    rcPolyMesh *poly_mesh = nullptr;
    rcPolyMeshDetail *detail_mesh = nullptr;
    if (solid == nullptr
        || !rcCreateHeightfield(
            &context,
            *solid,
            config.width,
            config.height,
            config.bmin,
            config.bmax,
            config.cs,
            config.ch)) {
        free_intermediates(solid, compact, contours, poly_mesh, detail_mesh);
        set_error(error, error_capacity, "failed to create Recast heightfield");
        return BF_NAVIGATION_COOK_RECAST_FAILED;
    }

    std::vector<unsigned char> triangle_areas(
        input.triangle_areas,
        input.triangle_areas + input.triangle_count);
    rcClearUnwalkableTriangles(
        &context,
        config.walkableSlopeAngle,
        input.vertices,
        static_cast<int>(input.vertex_count),
        reinterpret_cast<const int *>(input.indices),
        static_cast<int>(input.triangle_count),
        triangle_areas.data());
    if (!rcRasterizeTriangles(
            &context,
            input.vertices,
            static_cast<int>(input.vertex_count),
            reinterpret_cast<const int *>(input.indices),
            triangle_areas.data(),
            static_cast<int>(input.triangle_count),
            *solid,
            config.walkableClimb)) {
        free_intermediates(solid, compact, contours, poly_mesh, detail_mesh);
        set_error(error, error_capacity, "failed to rasterize navigation geometry");
        return BF_NAVIGATION_COOK_RECAST_FAILED;
    }
    rcFilterLedgeSpans(&context, config.walkableHeight, config.walkableClimb, *solid);
    rcFilterWalkableLowHeightSpans(&context, config.walkableHeight, *solid);

    compact = rcAllocCompactHeightfield();
    if (compact == nullptr
        || !rcBuildCompactHeightfield(
            &context,
            config.walkableHeight,
            config.walkableClimb,
            *solid,
            *compact)
        || !rcErodeWalkableArea(&context, config.walkableRadius, *compact)
        || !rcBuildDistanceField(&context, *compact)
        || !rcBuildRegions(
            &context,
            *compact,
            config.borderSize,
            config.minRegionArea,
            config.mergeRegionArea)) {
        free_intermediates(solid, compact, contours, poly_mesh, detail_mesh);
        set_error(error, error_capacity, "failed to build Recast compact regions");
        return BF_NAVIGATION_COOK_RECAST_FAILED;
    }

    contours = rcAllocContourSet();
    if (contours == nullptr
        || !rcBuildContours(
            &context,
            *compact,
            config.maxSimplificationError,
            config.maxEdgeLen,
            *contours)) {
        free_intermediates(solid, compact, contours, poly_mesh, detail_mesh);
        set_error(error, error_capacity, "failed to build Recast contours");
        return BF_NAVIGATION_COOK_RECAST_FAILED;
    }
    if (contours->nconts == 0) {
        free_intermediates(solid, compact, contours, poly_mesh, detail_mesh);
        return BF_NAVIGATION_COOK_OK;
    }

    poly_mesh = rcAllocPolyMesh();
    detail_mesh = rcAllocPolyMeshDetail();
    if (poly_mesh == nullptr || detail_mesh == nullptr
        || !rcBuildPolyMesh(
            &context,
            *contours,
            config.maxVertsPerPoly,
            *poly_mesh)
        || !rcBuildPolyMeshDetail(
            &context,
            *poly_mesh,
            *compact,
            config.detailSampleDist,
            config.detailSampleMaxError,
            *detail_mesh)) {
        free_intermediates(solid, compact, contours, poly_mesh, detail_mesh);
        set_error(error, error_capacity, "failed to build Recast polygon mesh");
        return BF_NAVIGATION_COOK_RECAST_FAILED;
    }
    if (poly_mesh->npolys == 0) {
        free_intermediates(solid, compact, contours, poly_mesh, detail_mesh);
        return BF_NAVIGATION_COOK_OK;
    }
    for (int polygon = 0; polygon < poly_mesh->npolys; ++polygon) {
        const uint8_t internal = poly_mesh->areas[polygon];
        if (internal == 0 || internal > 63) {
            poly_mesh->areas[polygon] = 0;
            poly_mesh->flags[polygon] = 0;
            continue;
        }
        const uint8_t authored = input.area_remap[internal];
        poly_mesh->areas[polygon] = authored;
        poly_mesh->flags[polygon] =
            input.area_traversable[authored] == 0 ? 0 : 1;
    }

    dtNavMeshCreateParams params {};
    params.verts = poly_mesh->verts;
    params.vertCount = poly_mesh->nverts;
    params.polys = poly_mesh->polys;
    params.polyAreas = poly_mesh->areas;
    params.polyFlags = poly_mesh->flags;
    params.polyCount = poly_mesh->npolys;
    params.nvp = poly_mesh->nvp;
    params.detailMeshes = detail_mesh->meshes;
    params.detailVerts = detail_mesh->verts;
    params.detailVertsCount = detail_mesh->nverts;
    params.detailTris = detail_mesh->tris;
    params.detailTriCount = detail_mesh->ntris;
    params.offMeshConVerts = input.off_mesh_vertices;
    params.offMeshConRad = input.off_mesh_radii;
    params.offMeshConDir = input.off_mesh_directions;
    params.offMeshConAreas = input.off_mesh_areas;
    params.offMeshConFlags = input.off_mesh_flags;
    params.offMeshConUserID = input.off_mesh_user_ids;
    params.offMeshConCount = static_cast<int>(input.off_mesh_count);
    params.walkableHeight = settings.agent_height;
    params.walkableRadius = settings.agent_radius;
    params.walkableClimb = settings.agent_max_climb;
    params.tileX = tile_x;
    params.tileY = tile_y;
    params.tileLayer = 0;
    rcVcopy(params.bmin, poly_mesh->bmin);
    rcVcopy(params.bmax, poly_mesh->bmax);
    params.cs = config.cs;
    params.ch = config.ch;
    params.buildBvTree = true;

    unsigned char *nav_data = nullptr;
    int nav_data_size = 0;
    const bool created = dtCreateNavMeshData(&params, &nav_data, &nav_data_size);
    if (!created || nav_data == nullptr || nav_data_size <= 0) {
        dtFree(nav_data);
        free_intermediates(solid, compact, contours, poly_mesh, detail_mesh);
        set_error(error, error_capacity, "failed to create Detour tile data");
        return BF_NAVIGATION_COOK_DETOUR_FAILED;
    }
    result.x = tile_x;
    result.y = tile_y;
    result.layer = 0;
    result.data.assign(nav_data, nav_data + nav_data_size);
    result.polygons =
        static_cast<uint32_t>(poly_mesh->npolys) + input.off_mesh_count;
    dtFree(nav_data);
    free_intermediates(solid, compact, contours, poly_mesh, detail_mesh);
    return BF_NAVIGATION_COOK_OK;
}

} // namespace

extern "C" int32_t bf_navigation_cooker_build(
    const BFNavigationCookSettings *settings,
    const BFNavigationCookInput *input,
    BFNavigationCookOutput *output,
    char *error,
    size_t error_capacity) {
    if (settings == nullptr || input == nullptr || output == nullptr
        || !valid_settings(*settings) || !valid_input(*input)) {
        set_error(error, error_capacity, "invalid navigation cooker input");
        return BF_NAVIGATION_COOK_INVALID_INPUT;
    }
    *output = BFNavigationCookOutput {};

    float bounds_min[3];
    float bounds_max[3];
    rcCalcBounds(
        input->vertices,
        static_cast<int>(input->vertex_count),
        bounds_min,
        bounds_max);
    bounds_min[1] -= settings->agent_max_climb + settings->cell_height * 2.0F;
    bounds_max[1] += settings->agent_height + settings->cell_height * 2.0F;
    int grid_width = 0;
    int grid_height = 0;
    rcCalcGridSize(
        bounds_min,
        bounds_max,
        settings->cell_size,
        &grid_width,
        &grid_height);
    const int tile_size = static_cast<int>(settings->tile_size);
    const int tiles_wide = (grid_width + tile_size - 1) / tile_size;
    const int tiles_high = (grid_height + tile_size - 1) / tile_size;
    if (tiles_wide <= 0 || tiles_high <= 0) {
        set_error(error, error_capacity, "navigation geometry has empty bounds");
        return BF_NAVIGATION_COOK_INVALID_INPUT;
    }

    const float tile_world = settings->tile_size * settings->cell_size;
    std::vector<TileBuild> built;
    uint32_t maximum_polygons = 0;
    for (int x = 0; x < tiles_wide; ++x) {
        for (int y = 0; y < tiles_high; ++y) {
            float tile_min[3] = {
                bounds_min[0] + static_cast<float>(x) * tile_world,
                bounds_min[1],
                bounds_min[2] + static_cast<float>(y) * tile_world,
            };
            float tile_max[3] = {
                bounds_min[0] + static_cast<float>(x + 1) * tile_world,
                bounds_max[1],
                bounds_min[2] + static_cast<float>(y + 1) * tile_world,
            };
            TileBuild tile {};
            const int32_t status = build_tile(
                *settings,
                *input,
                tile_min,
                tile_max,
                x,
                y,
                tile,
                error,
                error_capacity);
            if (status != BF_NAVIGATION_COOK_OK) {
                return status;
            }
            if (!tile.data.empty()) {
                maximum_polygons = std::max(maximum_polygons, tile.polygons);
                built.push_back(std::move(tile));
            }
        }
    }
    if (built.empty()) {
        set_error(error, error_capacity, "navigation geometry produced no walkable tiles");
        return BF_NAVIGATION_COOK_RECAST_FAILED;
    }

    const uint64_t grid_tiles =
        static_cast<uint64_t>(tiles_wide) * static_cast<uint64_t>(tiles_high);
    if (grid_tiles > std::numeric_limits<uint32_t>::max()) {
        set_error(error, error_capacity, "navigation tile grid exceeds Detour limits");
        return BF_NAVIGATION_COOK_INVALID_INPUT;
    }
    const uint32_t max_tiles = next_power_of_two(static_cast<uint32_t>(grid_tiles));
    const uint32_t max_polygons = next_power_of_two(maximum_polygons);
    if (max_tiles == 0 || max_polygons == 0
        || integer_log2(max_tiles) + integer_log2(max_polygons) > 22) {
        set_error(error, error_capacity, "navigation tile reference layout exceeds Detour limits");
        return BF_NAVIGATION_COOK_INVALID_INPUT;
    }

    auto *tiles = new (std::nothrow) BFNavigationCookedTile[built.size()] {};
    if (tiles == nullptr) {
        set_error(error, error_capacity, "failed to allocate cooked tile table");
        return BF_NAVIGATION_COOK_OUT_OF_MEMORY;
    }
    for (size_t index = 0; index < built.size(); ++index) {
        const TileBuild &source = built[index];
        auto *data = new (std::nothrow) uint8_t[source.data.size()];
        if (data == nullptr) {
            output->tiles = tiles;
            output->tile_count = static_cast<uint32_t>(index);
            bf_navigation_cooker_free(output);
            set_error(error, error_capacity, "failed to allocate cooked tile bytes");
            return BF_NAVIGATION_COOK_OUT_OF_MEMORY;
        }
        std::memcpy(data, source.data.data(), source.data.size());
        tiles[index] = BFNavigationCookedTile {
            source.x,
            source.y,
            source.layer,
            data,
            static_cast<uint32_t>(source.data.size()),
        };
    }
    rcVcopy(output->origin, bounds_min);
    output->tile_width = tile_world;
    output->tile_height = tile_world;
    output->max_tiles = max_tiles;
    output->max_polygons_per_tile = max_polygons;
    output->tiles = tiles;
    output->tile_count = static_cast<uint32_t>(built.size());
    return BF_NAVIGATION_COOK_OK;
}

extern "C" void bf_navigation_cooker_free(BFNavigationCookOutput *output) {
    if (output == nullptr) {
        return;
    }
    for (uint32_t index = 0; index < output->tile_count; ++index) {
        delete[] output->tiles[index].data;
    }
    delete[] output->tiles;
    *output = BFNavigationCookOutput {};
}
