#include "wrapper.h"

#include <embree4/rtcore.h>

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <new>

struct BFSpatialQueryDevice {
    RTCDevice handle;
};

struct BFSpatialQueryScene {
    BFSpatialQueryDevice *device;
    RTCScene handle;
    bool committed;
};

namespace {

bool finite(BFSpatialQueryVec3 value) {
    return std::isfinite(value.x) && std::isfinite(value.y) && std::isfinite(value.z);
}

int32_t native_status(BFSpatialQueryDevice *device) {
    if (!device || !device->handle) {
        return BF_SPATIAL_QUERY_STATUS_NULL_POINTER;
    }
    return rtcGetDeviceError(device->handle) == RTC_ERROR_NONE
        ? BF_SPATIAL_QUERY_STATUS_OK
        : BF_SPATIAL_QUERY_STATUS_NATIVE_FAILURE;
}

bool same_surface(
    const BFSpatialQuerySurfaceHit &left,
    const BFSpatialQuerySurfaceHit &right) {
    return left.geometry_id == right.geometry_id
        && left.primitive_id == right.primitive_id
        && left.instance_id == right.instance_id;
}

bool hit_less(
    const BFSpatialQuerySurfaceHit &left,
    const BFSpatialQuerySurfaceHit &right) {
    if (left.fraction != right.fraction) {
        return left.fraction < right.fraction;
    }
    if (left.geometry_id != right.geometry_id) {
        return left.geometry_id < right.geometry_id;
    }
    if (left.primitive_id != right.primitive_id) {
        return left.primitive_id < right.primitive_id;
    }
    return left.instance_id < right.instance_id;
}

struct SegmentQueryContext {
    RTCRayQueryContext context;
    BFSpatialQuerySurfaceHit *hits;
    uint32_t capacity;
    uint32_t count;
    float segment_length;
};

void collect_surface_hit(const RTCFilterFunctionNArguments *arguments) {
    if (!arguments || !arguments->valid || !arguments->context
        || !arguments->ray || !arguments->hit || arguments->N != 1) {
        return;
    }

    arguments->valid[0] = 0;
    auto *context = reinterpret_cast<SegmentQueryContext *>(arguments->context);
    auto *ray = reinterpret_cast<RTCRay *>(arguments->ray);
    auto *hit = reinterpret_cast<RTCHit *>(arguments->hit);
    if (!context->hits || context->capacity == 0 || !std::isfinite(ray->tfar)) {
        return;
    }

    BFSpatialQuerySurfaceHit candidate{};
    candidate.distance = ray->tfar * context->segment_length;
    candidate.fraction = ray->tfar;
    candidate.geometric_normal = {hit->Ng_x, hit->Ng_y, hit->Ng_z};
    candidate.barycentric_u = hit->u;
    candidate.barycentric_v = hit->v;
    candidate.geometry_id = hit->geomID;
    candidate.primitive_id = hit->primID;
    candidate.instance_id = hit->instID[0];

    for (uint32_t index = 0; index < context->count; ++index) {
        if (same_surface(context->hits[index], candidate)) {
            if (hit_less(candidate, context->hits[index])) {
                context->hits[index] = candidate;
            }
            return;
        }
    }

    if (context->count < context->capacity) {
        context->hits[context->count++] = candidate;
        return;
    }

    uint32_t worst = 0;
    for (uint32_t index = 1; index < context->count; ++index) {
        if (hit_less(context->hits[worst], context->hits[index])) {
            worst = index;
        }
    }
    if (hit_less(candidate, context->hits[worst])) {
        context->hits[worst] = candidate;
    }
}

}  // namespace

BFSpatialQueryVersion bf_spatial_query_embree_version(void) {
    return {RTC_VERSION_MAJOR, RTC_VERSION_MINOR, RTC_VERSION_PATCH};
}

int32_t bf_spatial_query_device_create(BFSpatialQueryDevice **out_device) {
    if (!out_device) {
        return BF_SPATIAL_QUERY_STATUS_NULL_POINTER;
    }
    *out_device = nullptr;
    auto *device = new (std::nothrow) BFSpatialQueryDevice{};
    if (!device) {
        return BF_SPATIAL_QUERY_STATUS_OUT_OF_MEMORY;
    }
    device->handle = rtcNewDevice("set_affinity=0");
    if (!device->handle) {
        delete device;
        return BF_SPATIAL_QUERY_STATUS_NATIVE_FAILURE;
    }
    *out_device = device;
    return BF_SPATIAL_QUERY_STATUS_OK;
}

void bf_spatial_query_device_destroy(BFSpatialQueryDevice *device) {
    if (!device) {
        return;
    }
    if (device->handle) {
        rtcReleaseDevice(device->handle);
    }
    delete device;
}

int32_t bf_spatial_query_scene_create(
    BFSpatialQueryDevice *device,
    BFSpatialQueryScene **out_scene) {
    if (!device || !device->handle || !out_scene) {
        return BF_SPATIAL_QUERY_STATUS_NULL_POINTER;
    }
    *out_scene = nullptr;
    auto *scene = new (std::nothrow) BFSpatialQueryScene{};
    if (!scene) {
        return BF_SPATIAL_QUERY_STATUS_OUT_OF_MEMORY;
    }
    scene->device = device;
    scene->handle = rtcNewScene(device->handle);
    if (!scene->handle) {
        delete scene;
        return BF_SPATIAL_QUERY_STATUS_NATIVE_FAILURE;
    }
    rtcSetSceneFlags(
        scene->handle,
        static_cast<RTCSceneFlags>(
            RTC_SCENE_FLAG_ROBUST | RTC_SCENE_FLAG_FILTER_FUNCTION_IN_ARGUMENTS));
    rtcSetSceneBuildQuality(scene->handle, RTC_BUILD_QUALITY_MEDIUM);
    scene->committed = false;
    const auto status = native_status(device);
    if (status != BF_SPATIAL_QUERY_STATUS_OK) {
        rtcReleaseScene(scene->handle);
        delete scene;
        return status;
    }
    *out_scene = scene;
    return BF_SPATIAL_QUERY_STATUS_OK;
}

void bf_spatial_query_scene_destroy(BFSpatialQueryScene *scene) {
    if (!scene) {
        return;
    }
    if (scene->handle) {
        rtcReleaseScene(scene->handle);
    }
    delete scene;
}

int32_t bf_spatial_query_scene_add_triangles(
    BFSpatialQueryScene *scene,
    const BFSpatialQueryTriangle *triangles,
    uint32_t triangle_count,
    uint32_t *out_geometry_id) {
    if (!scene || !scene->handle || !scene->device || !out_geometry_id) {
        return BF_SPATIAL_QUERY_STATUS_NULL_POINTER;
    }
    if (scene->committed) {
        return BF_SPATIAL_QUERY_STATUS_SCENE_COMMITTED;
    }
    if (!triangles || triangle_count == 0
        || triangle_count > UINT32_MAX / 3) {
        return BF_SPATIAL_QUERY_STATUS_INVALID_ARGUMENT;
    }
    for (uint32_t triangle = 0; triangle < triangle_count; ++triangle) {
        for (uint32_t vertex = 0; vertex < 3; ++vertex) {
            if (!finite(triangles[triangle].vertices[vertex])) {
                return BF_SPATIAL_QUERY_STATUS_INVALID_ARGUMENT;
            }
        }
    }

    RTCGeometry geometry = rtcNewGeometry(scene->device->handle, RTC_GEOMETRY_TYPE_TRIANGLE);
    if (!geometry) {
        return BF_SPATIAL_QUERY_STATUS_NATIVE_FAILURE;
    }
    const size_t vertex_count = static_cast<size_t>(triangle_count) * 3;
    auto *vertices = static_cast<BFSpatialQueryVec3 *>(rtcSetNewGeometryBuffer(
        geometry,
        RTC_BUFFER_TYPE_VERTEX,
        0,
        RTC_FORMAT_FLOAT3,
        sizeof(BFSpatialQueryVec3),
        vertex_count));
    auto *indices = static_cast<uint32_t *>(rtcSetNewGeometryBuffer(
        geometry,
        RTC_BUFFER_TYPE_INDEX,
        0,
        RTC_FORMAT_UINT3,
        sizeof(uint32_t) * 3,
        triangle_count));
    if (!vertices || !indices) {
        rtcReleaseGeometry(geometry);
        return BF_SPATIAL_QUERY_STATUS_NATIVE_FAILURE;
    }
    for (uint32_t triangle = 0; triangle < triangle_count; ++triangle) {
        const size_t base = static_cast<size_t>(triangle) * 3;
        vertices[base] = triangles[triangle].vertices[0];
        vertices[base + 1] = triangles[triangle].vertices[1];
        vertices[base + 2] = triangles[triangle].vertices[2];
        indices[base] = static_cast<uint32_t>(base);
        indices[base + 1] = static_cast<uint32_t>(base + 1);
        indices[base + 2] = static_cast<uint32_t>(base + 2);
    }

    rtcCommitGeometry(geometry);
    const uint32_t geometry_id = rtcAttachGeometry(scene->handle, geometry);
    rtcReleaseGeometry(geometry);
    const auto status = native_status(scene->device);
    if (status != BF_SPATIAL_QUERY_STATUS_OK
        || geometry_id == RTC_INVALID_GEOMETRY_ID) {
        return BF_SPATIAL_QUERY_STATUS_NATIVE_FAILURE;
    }
    *out_geometry_id = geometry_id;
    return BF_SPATIAL_QUERY_STATUS_OK;
}

int32_t bf_spatial_query_scene_commit(BFSpatialQueryScene *scene) {
    if (!scene || !scene->handle || !scene->device) {
        return BF_SPATIAL_QUERY_STATUS_NULL_POINTER;
    }
    if (scene->committed) {
        return BF_SPATIAL_QUERY_STATUS_SCENE_COMMITTED;
    }
    rtcCommitScene(scene->handle);
    const auto status = native_status(scene->device);
    if (status == BF_SPATIAL_QUERY_STATUS_OK) {
        scene->committed = true;
    }
    return status;
}

int32_t bf_spatial_query_scene_intersect_segment(
    const BFSpatialQueryScene *scene,
    BFSpatialQueryVec3 start,
    BFSpatialQueryVec3 end,
    uint32_t max_hits,
    BFSpatialQuerySurfaceHit *out_hits,
    uint32_t *out_hit_count) {
    if (!scene || !scene->handle || !scene->device || !out_hit_count) {
        return BF_SPATIAL_QUERY_STATUS_NULL_POINTER;
    }
    *out_hit_count = 0;
    if (!scene->committed || !finite(start) || !finite(end)) {
        return BF_SPATIAL_QUERY_STATUS_INVALID_ARGUMENT;
    }
    if (max_hits == 0) {
        return BF_SPATIAL_QUERY_STATUS_OK;
    }
    if (!out_hits) {
        return BF_SPATIAL_QUERY_STATUS_NULL_POINTER;
    }

    const BFSpatialQueryVec3 direction{
        end.x - start.x,
        end.y - start.y,
        end.z - start.z,
    };
    const float length_squared = direction.x * direction.x
        + direction.y * direction.y
        + direction.z * direction.z;
    if (!std::isfinite(length_squared)) {
        return BF_SPATIAL_QUERY_STATUS_INVALID_ARGUMENT;
    }
    if (length_squared <= 0.0f) {
        return BF_SPATIAL_QUERY_STATUS_OK;
    }

    SegmentQueryContext context{};
    rtcInitRayQueryContext(&context.context);
    context.hits = out_hits;
    context.capacity = max_hits;
    context.count = 0;
    context.segment_length = std::sqrt(length_squared);

    RTCRayHit ray_hit{};
    ray_hit.ray.org_x = start.x;
    ray_hit.ray.org_y = start.y;
    ray_hit.ray.org_z = start.z;
    ray_hit.ray.dir_x = direction.x;
    ray_hit.ray.dir_y = direction.y;
    ray_hit.ray.dir_z = direction.z;
    ray_hit.ray.tnear = 0.0f;
    ray_hit.ray.tfar = 1.0f;
    ray_hit.ray.time = 0.0f;
    ray_hit.ray.mask = UINT32_MAX;
    ray_hit.ray.id = 0;
    ray_hit.ray.flags = 0;
    ray_hit.hit.geomID = RTC_INVALID_GEOMETRY_ID;
    ray_hit.hit.primID = RTC_INVALID_GEOMETRY_ID;
    ray_hit.hit.instID[0] = RTC_INVALID_GEOMETRY_ID;

    RTCIntersectArguments arguments{};
    rtcInitIntersectArguments(&arguments);
    arguments.context = &context.context;
    arguments.filter = collect_surface_hit;
    arguments.flags = RTC_RAY_QUERY_FLAG_INVOKE_ARGUMENT_FILTER;
    rtcIntersect1(scene->handle, &ray_hit, &arguments);

    const auto status = native_status(scene->device);
    if (status != BF_SPATIAL_QUERY_STATUS_OK) {
        return status;
    }
    std::sort(out_hits, out_hits + context.count, hit_less);
    *out_hit_count = context.count;
    return BF_SPATIAL_QUERY_STATUS_OK;
}

int32_t bf_spatial_query_scene_closest_hit(
    const BFSpatialQueryScene *scene,
    BFSpatialQueryVec3 start,
    BFSpatialQueryVec3 end,
    BFSpatialQuerySurfaceHit *out_hit,
    uint8_t *out_has_hit) {
    if (!scene || !scene->handle || !scene->device
        || !out_hit || !out_has_hit) {
        return BF_SPATIAL_QUERY_STATUS_NULL_POINTER;
    }
    *out_hit = {};
    *out_has_hit = 0;
    if (!scene->committed || !finite(start) || !finite(end)) {
        return BF_SPATIAL_QUERY_STATUS_INVALID_ARGUMENT;
    }
    const BFSpatialQueryVec3 direction{
        end.x - start.x,
        end.y - start.y,
        end.z - start.z,
    };
    const float length_squared = direction.x * direction.x
        + direction.y * direction.y
        + direction.z * direction.z;
    if (!std::isfinite(length_squared)) {
        return BF_SPATIAL_QUERY_STATUS_INVALID_ARGUMENT;
    }
    if (length_squared <= 0.0f) {
        return BF_SPATIAL_QUERY_STATUS_OK;
    }

    RTCRayHit ray_hit{};
    ray_hit.ray.org_x = start.x;
    ray_hit.ray.org_y = start.y;
    ray_hit.ray.org_z = start.z;
    ray_hit.ray.dir_x = direction.x;
    ray_hit.ray.dir_y = direction.y;
    ray_hit.ray.dir_z = direction.z;
    ray_hit.ray.tnear = 0.0f;
    ray_hit.ray.tfar = 1.0f;
    ray_hit.ray.mask = UINT32_MAX;
    ray_hit.hit.geomID = RTC_INVALID_GEOMETRY_ID;
    ray_hit.hit.primID = RTC_INVALID_GEOMETRY_ID;
    ray_hit.hit.instID[0] = RTC_INVALID_GEOMETRY_ID;
    RTCIntersectArguments arguments{};
    rtcInitIntersectArguments(&arguments);
    rtcIntersect1(scene->handle, &ray_hit, &arguments);

    const auto status = native_status(scene->device);
    if (status != BF_SPATIAL_QUERY_STATUS_OK) {
        return status;
    }
    if (ray_hit.hit.geomID == RTC_INVALID_GEOMETRY_ID) {
        return BF_SPATIAL_QUERY_STATUS_OK;
    }
    const float segment_length = std::sqrt(length_squared);
    *out_hit = {
        ray_hit.ray.tfar * segment_length,
        ray_hit.ray.tfar,
        {ray_hit.hit.Ng_x, ray_hit.hit.Ng_y, ray_hit.hit.Ng_z},
        ray_hit.hit.u,
        ray_hit.hit.v,
        ray_hit.hit.geomID,
        ray_hit.hit.primID,
        ray_hit.hit.instID[0],
    };
    *out_has_hit = 1;
    return BF_SPATIAL_QUERY_STATUS_OK;
}

int32_t bf_spatial_query_scene_is_occluded(
    const BFSpatialQueryScene *scene,
    BFSpatialQueryVec3 start,
    BFSpatialQueryVec3 end,
    uint8_t *out_occluded) {
    if (!scene || !scene->handle || !scene->device || !out_occluded) {
        return BF_SPATIAL_QUERY_STATUS_NULL_POINTER;
    }
    *out_occluded = 0;
    if (!scene->committed || !finite(start) || !finite(end)) {
        return BF_SPATIAL_QUERY_STATUS_INVALID_ARGUMENT;
    }
    const BFSpatialQueryVec3 direction{
        end.x - start.x,
        end.y - start.y,
        end.z - start.z,
    };
    const float length_squared = direction.x * direction.x
        + direction.y * direction.y
        + direction.z * direction.z;
    if (!std::isfinite(length_squared)) {
        return BF_SPATIAL_QUERY_STATUS_INVALID_ARGUMENT;
    }
    if (length_squared <= 0.0f) {
        return BF_SPATIAL_QUERY_STATUS_OK;
    }

    RTCRay ray{};
    ray.org_x = start.x;
    ray.org_y = start.y;
    ray.org_z = start.z;
    ray.dir_x = direction.x;
    ray.dir_y = direction.y;
    ray.dir_z = direction.z;
    ray.tnear = 0.0f;
    ray.tfar = 1.0f;
    ray.mask = UINT32_MAX;
    RTCOccludedArguments arguments{};
    rtcInitOccludedArguments(&arguments);
    rtcOccluded1(scene->handle, &ray, &arguments);

    const auto status = native_status(scene->device);
    if (status != BF_SPATIAL_QUERY_STATUS_OK) {
        return status;
    }
    *out_occluded = ray.tfar < 0.0f ? 1 : 0;
    return BF_SPATIAL_QUERY_STATUS_OK;
}
