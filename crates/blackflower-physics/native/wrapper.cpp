#include "wrapper.h"

#include <Jolt/Jolt.h>

#include <Jolt/Core/Factory.h>
#include <Jolt/Core/JobSystemThreadPool.h>
#include <Jolt/Core/TempAllocator.h>
#include <Jolt/Physics/Body/BodyCreationSettings.h>
#include <Jolt/Physics/Collision/BroadPhase/BroadPhaseLayer.h>
#include <Jolt/Physics/Collision/ObjectLayer.h>
#include <Jolt/Physics/Collision/Shape/BoxShape.h>
#include <Jolt/Physics/Collision/Shape/SphereShape.h>
#include <Jolt/Physics/PhysicsSettings.h>
#include <Jolt/Physics/PhysicsSystem.h>
#include <Jolt/RegisterTypes.h>

#include <cmath>
#include <cstdint>
#include <mutex>
#include <new>

using namespace JPH;

namespace {

constexpr ObjectLayer kNonMovingLayer = 0;
constexpr ObjectLayer kMovingLayer = 1;
constexpr uint kLayerCount = 2;

class BroadPhaseLayers final : public BroadPhaseLayerInterface {
public:
    uint GetNumBroadPhaseLayers() const override {
        return kLayerCount;
    }

    BroadPhaseLayer GetBroadPhaseLayer(ObjectLayer layer) const override {
        return layers_[layer];
    }

private:
    BroadPhaseLayer layers_[kLayerCount] = {
        BroadPhaseLayer(0),
        BroadPhaseLayer(1),
    };
};

class ObjectVsBroadPhaseFilter final : public ObjectVsBroadPhaseLayerFilter {
public:
    bool ShouldCollide(ObjectLayer layer, BroadPhaseLayer broad_phase_layer) const override {
        if (layer == kNonMovingLayer) {
            return broad_phase_layer == BroadPhaseLayer(kMovingLayer);
        }
        return true;
    }
};

class LayerPairFilter final : public ObjectLayerPairFilter {
public:
    bool ShouldCollide(ObjectLayer first, ObjectLayer second) const override {
        if (first == kNonMovingLayer) {
            return second == kMovingLayer;
        }
        return true;
    }
};

std::mutex runtime_mutex;
uint32_t runtime_references = 0;

void acquire_runtime() {
    std::lock_guard lock(runtime_mutex);
    if (runtime_references == 0) {
        RegisterDefaultAllocator();
        Factory::sInstance = new Factory();
        RegisterTypes();
    }
    ++runtime_references;
}

void release_runtime() {
    std::lock_guard lock(runtime_mutex);
    --runtime_references;
    if (runtime_references == 0) {
        UnregisterTypes();
        delete Factory::sInstance;
        Factory::sInstance = nullptr;
    }
}

bool finite(BFPhysicsVec3 value) {
    return std::isfinite(value.x) && std::isfinite(value.y) && std::isfinite(value.z);
}

bool finite(BFPhysicsQuat value) {
    return std::isfinite(value.x) && std::isfinite(value.y) && std::isfinite(value.z)
        && std::isfinite(value.w);
}

bool valid_body_settings(const BFPhysicsBodySettings &settings) {
    return finite(settings.position) && finite(settings.rotation)
        && settings.motion_type <= BF_PHYSICS_MOTION_DYNAMIC;
}

EMotionType motion_type(uint32_t value) {
    switch (value) {
    case BF_PHYSICS_MOTION_STATIC:
        return EMotionType::Static;
    case BF_PHYSICS_MOTION_KINEMATIC:
        return EMotionType::Kinematic;
    default:
        return EMotionType::Dynamic;
    }
}

ObjectLayer object_layer(EMotionType type) {
    return type == EMotionType::Static ? kNonMovingLayer : kMovingLayer;
}

EActivation activation(const BFPhysicsBodySettings &settings) {
    return settings.active == 0 ? EActivation::DontActivate : EActivation::Activate;
}

template <typename Vector>
BFPhysicsVec3 from_jolt(const Vector &value) {
    return BFPhysicsVec3 {
        static_cast<float>(value.GetX()),
        static_cast<float>(value.GetY()),
        static_cast<float>(value.GetZ()),
    };
}

int32_t require_body(const BFPhysicsWorld *world, BodyID body_id);

} // namespace

struct BFPhysicsWorld {
    explicit BFPhysicsWorld(const BFPhysicsWorldConfig &config)
        : job_system(cMaxPhysicsJobs, cMaxPhysicsBarriers, config.worker_threads) {
        system.Init(
            config.max_bodies,
            config.body_mutexes,
            config.max_body_pairs,
            config.max_contact_constraints,
            broad_phase_layers,
            object_vs_broad_phase_filter,
            object_layer_filter);
    }

    BroadPhaseLayers broad_phase_layers;
    ObjectVsBroadPhaseFilter object_vs_broad_phase_filter;
    LayerPairFilter object_layer_filter;
    TempAllocatorMalloc temp_allocator;
    JobSystemThreadPool job_system;
    PhysicsSystem system;
};

namespace {

int32_t require_body(const BFPhysicsWorld *world, BodyID body_id) {
    if (!world->system.GetBodyInterface().IsAdded(body_id)) {
        return BF_PHYSICS_STATUS_BODY_NOT_FOUND;
    }
    return BF_PHYSICS_STATUS_OK;
}

int32_t create_body(
    BFPhysicsWorld *world,
    const BFPhysicsBodySettings *settings,
    const Shape *shape,
    uint32_t *out_body_id) {
    ShapeRefC owned_shape(shape);
    if (world == nullptr || settings == nullptr || shape == nullptr || out_body_id == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!valid_body_settings(*settings)) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }

    const EMotionType type = motion_type(settings->motion_type);
    BodyCreationSettings creation_settings(
        owned_shape,
        RVec3(settings->position.x, settings->position.y, settings->position.z),
        Quat(settings->rotation.x, settings->rotation.y, settings->rotation.z, settings->rotation.w),
        type,
        object_layer(type));
    BodyID body_id =
        world->system.GetBodyInterface().CreateAndAddBody(creation_settings, activation(*settings));
    if (body_id.IsInvalid()) {
        return BF_PHYSICS_STATUS_BODY_CAPACITY_EXHAUSTED;
    }

    *out_body_id = body_id.GetIndexAndSequenceNumber();
    return BF_PHYSICS_STATUS_OK;
}

} // namespace

extern "C" BFPhysicsVersion bf_physics_jolt_version() {
    return BFPhysicsVersion {
        JPH_VERSION_MAJOR,
        JPH_VERSION_MINOR,
        JPH_VERSION_PATCH,
    };
}

extern "C" int32_t bf_physics_world_create(
    const BFPhysicsWorldConfig *config,
    BFPhysicsWorld **out_world) {
    if (config == nullptr || out_world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (config->max_bodies == 0 || config->max_bodies > PhysicsSystem::cMaxBodiesLimit
        || config->max_body_pairs == 0
        || config->max_body_pairs > PhysicsSystem::cMaxBodyPairsLimit
        || config->max_contact_constraints == 0
        || config->max_contact_constraints > PhysicsSystem::cMaxContactConstraintsLimit
        || config->worker_threads < 0) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }

    acquire_runtime();
    BFPhysicsWorld *world = new (std::nothrow) BFPhysicsWorld(*config);
    if (world == nullptr) {
        release_runtime();
        return BF_PHYSICS_STATUS_INITIALIZATION_FAILED;
    }
    *out_world = world;
    return BF_PHYSICS_STATUS_OK;
}

extern "C" void bf_physics_world_destroy(BFPhysicsWorld *world) {
    if (world == nullptr) {
        return;
    }
    delete world;
    release_runtime();
}

extern "C" int32_t bf_physics_world_create_sphere_body(
    BFPhysicsWorld *world,
    const BFPhysicsBodySettings *settings,
    float radius,
    uint32_t *out_body_id) {
    if (!std::isfinite(radius) || radius <= 0.0F) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    return create_body(world, settings, new SphereShape(radius), out_body_id);
}

extern "C" int32_t bf_physics_world_create_box_body(
    BFPhysicsWorld *world,
    const BFPhysicsBodySettings *settings,
    BFPhysicsVec3 half_extent,
    uint32_t *out_body_id) {
    if (!finite(half_extent) || half_extent.x <= 0.0F || half_extent.y <= 0.0F
        || half_extent.z <= 0.0F) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    return create_body(
        world,
        settings,
        new BoxShape(Vec3(half_extent.x, half_extent.y, half_extent.z)),
        out_body_id);
}

extern "C" int32_t bf_physics_world_destroy_body(BFPhysicsWorld *world, uint32_t body_id) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    BodyInterface &body_interface = world->system.GetBodyInterface();
    body_interface.RemoveBody(id);
    body_interface.DestroyBody(id);
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_body_exists(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    uint8_t *out_exists) {
    if (world == nullptr || out_exists == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    *out_exists = world->system.GetBodyInterface().IsAdded(BodyID(body_id)) ? 1 : 0;
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_body_is_active(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    uint8_t *out_active) {
    if (world == nullptr || out_active == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    *out_active = world->system.GetBodyInterface().IsActive(id) ? 1 : 0;
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_body_position(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 *out_position) {
    if (world == nullptr || out_position == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    *out_position = from_jolt(world->system.GetBodyInterface().GetPosition(id));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_body_linear_velocity(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 *out_velocity) {
    if (world == nullptr || out_velocity == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    *out_velocity = from_jolt(world->system.GetBodyInterface().GetLinearVelocity(id));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_set_body_linear_velocity(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 velocity) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!finite(velocity)) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    world->system.GetBodyInterface().SetLinearVelocity(
        id,
        Vec3(velocity.x, velocity.y, velocity.z));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" void bf_physics_world_optimize_broad_phase(BFPhysicsWorld *world) {
    if (world != nullptr) {
        world->system.OptimizeBroadPhase();
    }
}

extern "C" int32_t bf_physics_world_update(
    BFPhysicsWorld *world,
    float delta_seconds,
    int32_t collision_steps,
    uint32_t *out_update_errors) {
    if (world == nullptr || out_update_errors == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!std::isfinite(delta_seconds) || delta_seconds <= 0.0F || collision_steps <= 0) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    *out_update_errors = static_cast<uint32_t>(world->system.Update(
        delta_seconds,
        collision_steps,
        &world->temp_allocator,
        &world->job_system));
    return BF_PHYSICS_STATUS_OK;
}
