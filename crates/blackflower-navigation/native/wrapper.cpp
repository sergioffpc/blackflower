#include "wrapper.h"

#include <DetourAlloc.h>
#include <DetourNavMesh.h>
#include <DetourNavMeshQuery.h>
#include <DetourStatus.h>

#include <cmath>
#include <cstring>
#include <limits>
#include <new>
#include <type_traits>

static_assert(std::is_same<BFNavigationPolyRef, dtPolyRef>::value, "poly references must match");
static_assert(std::is_same<BFNavigationTileRef, dtTileRef>::value, "tile references must match");
static_assert(sizeof(BFNavigationVec3) == sizeof(float) * 3, "vectors must be tightly packed");

struct BFNavigationNavMesh {
    dtNavMesh *value;
};

struct BFNavigationQuery {
    dtNavMeshQuery *value;
};

namespace {

bool finite(BFNavigationVec3 value) {
    return std::isfinite(value.x) && std::isfinite(value.y) && std::isfinite(value.z);
}

void copy_vector(BFNavigationVec3 value, float (&out)[3]) {
    out[0] = value.x;
    out[1] = value.y;
    out[2] = value.z;
}

BFNavigationVec3 copy_vector(const float *value) {
    return BFNavigationVec3 {value[0], value[1], value[2]};
}

bool positive_extents(BFNavigationVec3 value) {
    return finite(value) && value.x > 0.0F && value.y > 0.0F && value.z > 0.0F;
}

bool append_aligned_size(size_t count, size_t element_size, size_t &total) {
    const size_t maximum = std::numeric_limits<size_t>::max();
    if (element_size != 0 && count > maximum / element_size) {
        return false;
    }
    const size_t size = count * element_size;
    if (size > maximum - 3) {
        return false;
    }
    const size_t aligned = (size + 3) & ~size_t(3);
    if (total > maximum - aligned) {
        return false;
    }
    total += aligned;
    return true;
}

bool valid_tile_data(const uint8_t *data, size_t data_size) {
    if (data == nullptr || data_size < sizeof(dtMeshHeader)
        || data_size > static_cast<size_t>(std::numeric_limits<int>::max())) {
        return false;
    }

    dtMeshHeader header {};
    std::memcpy(&header, data, sizeof(header));
    if (header.magic != DT_NAVMESH_MAGIC || header.version != DT_NAVMESH_VERSION
        || header.polyCount <= 0 || header.vertCount < 3 || header.maxLinkCount <= 0
        || header.detailMeshCount < 0 || header.detailVertCount < 0 || header.detailTriCount < 0
        || header.bvNodeCount < 0 || header.offMeshConCount < 0) {
        return false;
    }

    size_t required = 0;
    return append_aligned_size(1, sizeof(dtMeshHeader), required)
        && append_aligned_size(static_cast<size_t>(header.vertCount), sizeof(float) * 3, required)
        && append_aligned_size(static_cast<size_t>(header.polyCount), sizeof(dtPoly), required)
        && append_aligned_size(static_cast<size_t>(header.maxLinkCount), sizeof(dtLink), required)
        && append_aligned_size(
            static_cast<size_t>(header.detailMeshCount),
            sizeof(dtPolyDetail),
            required)
        && append_aligned_size(
            static_cast<size_t>(header.detailVertCount),
            sizeof(float) * 3,
            required)
        && append_aligned_size(
            static_cast<size_t>(header.detailTriCount),
            sizeof(unsigned char) * 4,
            required)
        && append_aligned_size(static_cast<size_t>(header.bvNodeCount), sizeof(dtBVNode), required)
        && append_aligned_size(
            static_cast<size_t>(header.offMeshConCount),
            sizeof(dtOffMeshConnection),
            required)
        && required <= data_size;
}

unsigned char *copy_tile_data(const uint8_t *data, size_t data_size) {
    auto *copy = static_cast<unsigned char *>(dtAlloc(data_size, DT_ALLOC_PERM));
    if (copy != nullptr) {
        std::memcpy(copy, data, data_size);
    }
    return copy;
}

bool valid_params(const BFNavigationNavMeshParams &params) {
    return finite(params.origin) && std::isfinite(params.tile_width)
        && std::isfinite(params.tile_height) && params.tile_width > 0.0F
        && params.tile_height > 0.0F && params.max_tiles > 0
        && params.max_tiles <= static_cast<uint32_t>(std::numeric_limits<int>::max())
        && params.max_polygons_per_tile > 0
        && params.max_polygons_per_tile
            <= static_cast<uint32_t>(std::numeric_limits<int>::max());
}

bool valid_filter(const BFNavigationFilter &filter) {
    for (float cost : filter.area_costs) {
        if (!std::isfinite(cost) || cost <= 0.0F) {
            return false;
        }
    }
    return true;
}

dtQueryFilter make_filter(const BFNavigationFilter &source) {
    dtQueryFilter filter;
    filter.setIncludeFlags(source.include_flags);
    filter.setExcludeFlags(source.exclude_flags);
    for (int area = 0; area < BF_NAVIGATION_MAX_AREAS; ++area) {
        filter.setAreaCost(area, source.area_costs[area]);
    }
    return filter;
}

uint32_t status_details(dtStatus status) {
    uint32_t details = 0;
    if (dtStatusDetail(status, DT_BUFFER_TOO_SMALL)) {
        details |= BF_NAVIGATION_DETAIL_BUFFER_TOO_SMALL;
    }
    if (dtStatusDetail(status, DT_OUT_OF_NODES)) {
        details |= BF_NAVIGATION_DETAIL_OUT_OF_NODES;
    }
    if (dtStatusDetail(status, DT_PARTIAL_RESULT)) {
        details |= BF_NAVIGATION_DETAIL_PARTIAL_RESULT;
    }
    return details;
}

int32_t map_status(dtStatus status) {
    if (dtStatusSucceed(status)) {
        return BF_NAVIGATION_STATUS_OK;
    }
    if (dtStatusDetail(status, DT_WRONG_MAGIC) || dtStatusDetail(status, DT_WRONG_VERSION)) {
        return BF_NAVIGATION_STATUS_INVALID_NAVMESH_DATA;
    }
    if (dtStatusDetail(status, DT_OUT_OF_MEMORY)) {
        return BF_NAVIGATION_STATUS_OUT_OF_MEMORY;
    }
    if (dtStatusDetail(status, DT_ALREADY_OCCUPIED)) {
        return BF_NAVIGATION_STATUS_TILE_ALREADY_OCCUPIED;
    }
    if (dtStatusDetail(status, DT_INVALID_PARAM)) {
        return BF_NAVIGATION_STATUS_INVALID_ARGUMENT;
    }
    return BF_NAVIGATION_STATUS_QUERY_FAILED;
}

int32_t own_navmesh(dtNavMesh *value, BFNavigationNavMesh **out_navmesh) {
    auto *navmesh = new (std::nothrow) BFNavigationNavMesh {value};
    if (navmesh == nullptr) {
        dtFreeNavMesh(value);
        return BF_NAVIGATION_STATUS_OUT_OF_MEMORY;
    }
    *out_navmesh = navmesh;
    return BF_NAVIGATION_STATUS_OK;
}

int32_t restore_tile(
    dtNavMesh *navmesh,
    BFNavigationTileRef reference,
    unsigned char *data,
    int data_size,
    int32_t replacement_status) {
    BFNavigationTileRef restored_reference = 0;
    const dtStatus status = navmesh->addTile(
        data,
        data_size,
        DT_TILE_FREE_DATA,
        reference,
        &restored_reference);
    if (dtStatusSucceed(status) && restored_reference == reference) {
        return replacement_status;
    }
    if (dtStatusFailed(status)) {
        dtFree(data);
    }
    return BF_NAVIGATION_STATUS_QUERY_FAILED;
}

int32_t replace_tile_data(
    dtNavMesh *navmesh,
    BFNavigationTileRef reference,
    const uint8_t *data,
    size_t data_size,
    BFNavigationTileRef *out_reference) {
    const dtMeshTile *old_tile = navmesh->getTileByRef(reference);
    if (old_tile == nullptr) {
        return BF_NAVIGATION_STATUS_INVALID_ARGUMENT;
    }
    const int old_data_size = old_tile->dataSize;
    unsigned char *old_data =
        copy_tile_data(old_tile->data, static_cast<size_t>(old_data_size));
    unsigned char *new_data = copy_tile_data(data, data_size);
    if (old_data == nullptr || new_data == nullptr) {
        dtFree(old_data);
        dtFree(new_data);
        return BF_NAVIGATION_STATUS_OUT_OF_MEMORY;
    }
    dtStatus status = navmesh->removeTile(reference, nullptr, nullptr);
    if (dtStatusFailed(status)) {
        dtFree(old_data);
        dtFree(new_data);
        return map_status(status);
    }
    status = navmesh->addTile(
        new_data,
        static_cast<int>(data_size),
        DT_TILE_FREE_DATA,
        reference,
        out_reference);
    if (dtStatusSucceed(status) && *out_reference == reference) {
        dtFree(old_data);
        return BF_NAVIGATION_STATUS_OK;
    }
    const int32_t replacement_status =
        dtStatusFailed(status) ? map_status(status) : BF_NAVIGATION_STATUS_QUERY_FAILED;
    if (dtStatusSucceed(status)) {
        navmesh->removeTile(*out_reference, nullptr, nullptr);
        *out_reference = 0;
    } else {
        dtFree(new_data);
    }
    return restore_tile(navmesh, reference, old_data, old_data_size, replacement_status);
}

} // namespace

extern "C" BFNavigationVersion bf_navigation_recast_version() {
    return BFNavigationVersion {
        BF_RECAST_VERSION_MAJOR,
        BF_RECAST_VERSION_MINOR,
        BF_RECAST_VERSION_PATCH,
    };
}

extern "C" uint32_t bf_navigation_detour_navmesh_version() {
    return DT_NAVMESH_VERSION;
}

extern "C" int32_t bf_navigation_navmesh_create_single_tile(
    const uint8_t *data,
    size_t data_size,
    BFNavigationNavMesh **out_navmesh) {
    if (out_navmesh == nullptr) {
        return BF_NAVIGATION_STATUS_NULL_POINTER;
    }
    *out_navmesh = nullptr;
    if (!valid_tile_data(data, data_size)) {
        return BF_NAVIGATION_STATUS_INVALID_NAVMESH_DATA;
    }

    unsigned char *owned_data = copy_tile_data(data, data_size);
    if (owned_data == nullptr) {
        return BF_NAVIGATION_STATUS_OUT_OF_MEMORY;
    }
    dtNavMesh *value = dtAllocNavMesh();
    if (value == nullptr) {
        dtFree(owned_data);
        return BF_NAVIGATION_STATUS_OUT_OF_MEMORY;
    }

    const dtStatus status =
        value->init(owned_data, static_cast<int>(data_size), DT_TILE_FREE_DATA);
    if (dtStatusFailed(status)) {
        dtFree(owned_data);
        dtFreeNavMesh(value);
        return map_status(status);
    }
    return own_navmesh(value, out_navmesh);
}

extern "C" int32_t bf_navigation_navmesh_create_tiled(
    const BFNavigationNavMeshParams *params,
    BFNavigationNavMesh **out_navmesh) {
    if (params == nullptr || out_navmesh == nullptr) {
        return BF_NAVIGATION_STATUS_NULL_POINTER;
    }
    *out_navmesh = nullptr;
    if (!valid_params(*params)) {
        return BF_NAVIGATION_STATUS_INVALID_ARGUMENT;
    }

    dtNavMeshParams native_params {};
    copy_vector(params->origin, native_params.orig);
    native_params.tileWidth = params->tile_width;
    native_params.tileHeight = params->tile_height;
    native_params.maxTiles = static_cast<int>(params->max_tiles);
    native_params.maxPolys = static_cast<int>(params->max_polygons_per_tile);

    dtNavMesh *value = dtAllocNavMesh();
    if (value == nullptr) {
        return BF_NAVIGATION_STATUS_OUT_OF_MEMORY;
    }
    const dtStatus status = value->init(&native_params);
    if (dtStatusFailed(status)) {
        dtFreeNavMesh(value);
        return map_status(status);
    }
    return own_navmesh(value, out_navmesh);
}

extern "C" void bf_navigation_navmesh_destroy(BFNavigationNavMesh *navmesh) {
    if (navmesh != nullptr) {
        dtFreeNavMesh(navmesh->value);
        delete navmesh;
    }
}

extern "C" int32_t bf_navigation_navmesh_add_tile(
    BFNavigationNavMesh *navmesh,
    const uint8_t *data,
    size_t data_size,
    BFNavigationTileRef desired_reference,
    BFNavigationTileRef *out_reference) {
    if (navmesh == nullptr || out_reference == nullptr) {
        return BF_NAVIGATION_STATUS_NULL_POINTER;
    }
    *out_reference = 0;
    if (!valid_tile_data(data, data_size)) {
        return BF_NAVIGATION_STATUS_INVALID_NAVMESH_DATA;
    }

    unsigned char *owned_data = copy_tile_data(data, data_size);
    if (owned_data == nullptr) {
        return BF_NAVIGATION_STATUS_OUT_OF_MEMORY;
    }
    const dtStatus status = navmesh->value->addTile(
        owned_data,
        static_cast<int>(data_size),
        DT_TILE_FREE_DATA,
        desired_reference,
        out_reference);
    if (dtStatusFailed(status)) {
        dtFree(owned_data);
    }
    return map_status(status);
}

extern "C" int32_t bf_navigation_navmesh_remove_tile(
    BFNavigationNavMesh *navmesh,
    BFNavigationTileRef reference) {
    if (navmesh == nullptr) {
        return BF_NAVIGATION_STATUS_NULL_POINTER;
    }
    return map_status(navmesh->value->removeTile(reference, nullptr, nullptr));
}

extern "C" int32_t bf_navigation_navmesh_replace_tile(
    BFNavigationNavMesh *navmesh,
    BFNavigationTileRef reference,
    const uint8_t *data,
    size_t data_size,
    BFNavigationTileRef *out_reference) {
    if (navmesh == nullptr || out_reference == nullptr) {
        return BF_NAVIGATION_STATUS_NULL_POINTER;
    }
    *out_reference = 0;
    if (!valid_tile_data(data, data_size)) {
        return BF_NAVIGATION_STATUS_INVALID_NAVMESH_DATA;
    }
    return replace_tile_data(navmesh->value, reference, data, data_size, out_reference);
}

extern "C" int32_t bf_navigation_query_create(
    const BFNavigationNavMesh *navmesh,
    uint32_t max_nodes,
    BFNavigationQuery **out_query) {
    if (navmesh == nullptr || out_query == nullptr) {
        return BF_NAVIGATION_STATUS_NULL_POINTER;
    }
    *out_query = nullptr;
    if (max_nodes == 0 || max_nodes > 65'535) {
        return BF_NAVIGATION_STATUS_INVALID_ARGUMENT;
    }

    dtNavMeshQuery *value = dtAllocNavMeshQuery();
    if (value == nullptr) {
        return BF_NAVIGATION_STATUS_OUT_OF_MEMORY;
    }
    const dtStatus status = value->init(navmesh->value, static_cast<int>(max_nodes));
    if (dtStatusFailed(status)) {
        dtFreeNavMeshQuery(value);
        return map_status(status);
    }

    auto *query = new (std::nothrow) BFNavigationQuery {value};
    if (query == nullptr) {
        dtFreeNavMeshQuery(value);
        return BF_NAVIGATION_STATUS_OUT_OF_MEMORY;
    }
    *out_query = query;
    return BF_NAVIGATION_STATUS_OK;
}

extern "C" void bf_navigation_query_destroy(BFNavigationQuery *query) {
    if (query != nullptr) {
        dtFreeNavMeshQuery(query->value);
        delete query;
    }
}

extern "C" int32_t bf_navigation_query_find_nearest_point(
    const BFNavigationQuery *query,
    BFNavigationVec3 center,
    BFNavigationVec3 half_extents,
    const BFNavigationFilter *filter,
    BFNavigationNearestPoint *out_nearest) {
    if (query == nullptr || filter == nullptr || out_nearest == nullptr) {
        return BF_NAVIGATION_STATUS_NULL_POINTER;
    }
    *out_nearest = BFNavigationNearestPoint {};
    if (!finite(center) || !positive_extents(half_extents) || !valid_filter(*filter)) {
        return BF_NAVIGATION_STATUS_INVALID_ARGUMENT;
    }

    float native_center[3];
    float native_extents[3];
    float nearest[3] = {0.0F, 0.0F, 0.0F};
    copy_vector(center, native_center);
    copy_vector(half_extents, native_extents);
    dtPolyRef polygon = 0;
    bool is_over_polygon = false;
    const dtQueryFilter native_filter = make_filter(*filter);
    const dtStatus status = query->value->findNearestPoly(
        native_center,
        native_extents,
        &native_filter,
        &polygon,
        nearest,
        &is_over_polygon);
    if (dtStatusSucceed(status) && polygon != 0) {
        out_nearest->polygon = polygon;
        out_nearest->position = copy_vector(nearest);
        out_nearest->is_over_polygon = is_over_polygon ? 1 : 0;
    }
    return map_status(status);
}

extern "C" int32_t bf_navigation_query_closest_point_on_polygon(
    const BFNavigationQuery *query,
    BFNavigationPolyRef polygon,
    BFNavigationVec3 position,
    BFNavigationNearestPoint *out_closest) {
    if (query == nullptr || out_closest == nullptr) {
        return BF_NAVIGATION_STATUS_NULL_POINTER;
    }
    *out_closest = BFNavigationNearestPoint {};
    if (polygon == 0 || !finite(position)) {
        return BF_NAVIGATION_STATUS_INVALID_ARGUMENT;
    }

    float native_position[3];
    float closest[3] = {0.0F, 0.0F, 0.0F};
    copy_vector(position, native_position);
    bool is_over_polygon = false;
    const dtStatus status =
        query->value->closestPointOnPoly(polygon, native_position, closest, &is_over_polygon);
    if (dtStatusSucceed(status)) {
        out_closest->polygon = polygon;
        out_closest->position = copy_vector(closest);
        out_closest->is_over_polygon = is_over_polygon ? 1 : 0;
    }
    return map_status(status);
}

extern "C" int32_t bf_navigation_query_find_path(
    const BFNavigationQuery *query,
    BFNavigationPolyRef start_polygon,
    BFNavigationPolyRef end_polygon,
    BFNavigationVec3 start,
    BFNavigationVec3 end,
    const BFNavigationFilter *filter,
    BFNavigationPolyRef *out_path,
    uint32_t path_capacity,
    uint32_t *out_path_count,
    uint32_t *out_details) {
    if (query == nullptr || filter == nullptr || out_path == nullptr || out_path_count == nullptr
        || out_details == nullptr) {
        return BF_NAVIGATION_STATUS_NULL_POINTER;
    }
    *out_path_count = 0;
    *out_details = 0;
    if (start_polygon == 0 || end_polygon == 0 || !finite(start) || !finite(end)
        || !valid_filter(*filter) || path_capacity == 0
        || path_capacity > static_cast<uint32_t>(std::numeric_limits<int>::max())) {
        return BF_NAVIGATION_STATUS_INVALID_ARGUMENT;
    }

    float native_start[3];
    float native_end[3];
    copy_vector(start, native_start);
    copy_vector(end, native_end);
    const dtQueryFilter native_filter = make_filter(*filter);
    int path_count = 0;
    const dtStatus status = query->value->findPath(
        start_polygon,
        end_polygon,
        native_start,
        native_end,
        &native_filter,
        out_path,
        &path_count,
        static_cast<int>(path_capacity));
    *out_path_count = static_cast<uint32_t>(path_count);
    *out_details = status_details(status);
    return map_status(status);
}

extern "C" int32_t bf_navigation_query_find_straight_path(
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
    uint32_t *out_details) {
    if (query == nullptr || path == nullptr || out_points == nullptr || out_point_flags == nullptr
        || out_point_polygons == nullptr || out_point_count == nullptr || out_details == nullptr) {
        return BF_NAVIGATION_STATUS_NULL_POINTER;
    }
    *out_point_count = 0;
    *out_details = 0;
    if (!finite(start) || !finite(end) || path_count == 0
        || path_count > static_cast<uint32_t>(std::numeric_limits<int>::max())
        || point_capacity == 0
        || point_capacity > static_cast<uint32_t>(std::numeric_limits<int>::max())) {
        return BF_NAVIGATION_STATUS_INVALID_ARGUMENT;
    }

    float native_start[3];
    float native_end[3];
    copy_vector(start, native_start);
    copy_vector(end, native_end);
    int point_count = 0;
    const dtStatus status = query->value->findStraightPath(
        native_start,
        native_end,
        path,
        static_cast<int>(path_count),
        &out_points[0].x,
        out_point_flags,
        out_point_polygons,
        &point_count,
        static_cast<int>(point_capacity));
    *out_point_count = static_cast<uint32_t>(point_count);
    *out_details = status_details(status);
    return map_status(status);
}

extern "C" int32_t bf_navigation_query_raycast(
    const BFNavigationQuery *query,
    BFNavigationPolyRef start_polygon,
    BFNavigationVec3 start,
    BFNavigationVec3 end,
    const BFNavigationFilter *filter,
    BFNavigationPolyRef *out_visited,
    uint32_t visited_capacity,
    uint32_t *out_visited_count,
    uint32_t *out_details,
    BFNavigationRaycastResult *out_result) {
    if (query == nullptr || filter == nullptr || out_visited == nullptr
        || out_visited_count == nullptr || out_details == nullptr || out_result == nullptr) {
        return BF_NAVIGATION_STATUS_NULL_POINTER;
    }
    *out_visited_count = 0;
    *out_details = 0;
    *out_result = BFNavigationRaycastResult {};
    if (start_polygon == 0 || !finite(start) || !finite(end) || !valid_filter(*filter)
        || visited_capacity == 0
        || visited_capacity > static_cast<uint32_t>(std::numeric_limits<int>::max())) {
        return BF_NAVIGATION_STATUS_INVALID_ARGUMENT;
    }

    float native_start[3];
    float native_end[3];
    copy_vector(start, native_start);
    copy_vector(end, native_end);
    const dtQueryFilter native_filter = make_filter(*filter);
    dtRaycastHit hit {};
    hit.path = out_visited;
    hit.maxPath = static_cast<int>(visited_capacity);
    const dtStatus status = query->value->raycast(
        start_polygon,
        native_start,
        native_end,
        &native_filter,
        DT_RAYCAST_USE_COSTS,
        &hit);
    *out_visited_count = static_cast<uint32_t>(hit.pathCount);
    *out_details = status_details(status);
    if (dtStatusSucceed(status)) {
        out_result->fraction = hit.t;
        out_result->normal = copy_vector(hit.hitNormal);
        out_result->edge_index = hit.hitEdgeIndex;
        out_result->path_cost = hit.pathCost;
    }
    return map_status(status);
}
