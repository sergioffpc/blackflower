#include "wrapper.h"

#include <Jolt/Jolt.h>

#include <Jolt/Core/Factory.h>
#include <Jolt/Core/JobSystemThreadPool.h>
#include <Jolt/Core/TempAllocator.h>
#include <Jolt/Physics/Body/BodyCreationSettings.h>
#include <Jolt/Physics/Character/Character.h>
#include <Jolt/Physics/Collision/BroadPhase/BroadPhaseLayer.h>
#include <Jolt/Physics/Collision/ContactListener.h>
#include <Jolt/Physics/Collision/ObjectLayer.h>
#include <Jolt/Physics/Collision/Shape/BoxShape.h>
#include <Jolt/Physics/Collision/Shape/CapsuleShape.h>
#include <Jolt/Physics/Collision/Shape/RotatedTranslatedShape.h>
#include <Jolt/Physics/Collision/Shape/SphereShape.h>
#include <Jolt/Physics/PhysicsSettings.h>
#include <Jolt/Physics/PhysicsSystem.h>
#include <Jolt/RegisterTypes.h>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <mutex>
#include <new>
#include <tuple>
#include <unordered_map>
#include <vector>

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

Quat to_quat(BFPhysicsQuat value);

bool valid_body_settings(const BFPhysicsBodySettings &settings) {
    return finite(settings.position) && finite(settings.rotation)
        && to_quat(settings.rotation).IsNormalized()
        && settings.motion_type <= BF_PHYSICS_MOTION_DYNAMIC;
}

bool valid_character_settings(const BFPhysicsCharacterSettings &settings) {
    constexpr float half_pi = 1.57079632679F;
    return finite(settings.position) && finite(settings.rotation)
        && to_quat(settings.rotation).IsNormalized()
        && std::isfinite(settings.capsule_half_height) && settings.capsule_half_height > 0.0F
        && std::isfinite(settings.capsule_radius) && settings.capsule_radius > 0.0F
        && std::isfinite(settings.mass) && settings.mass > 0.0F
        && std::isfinite(settings.friction) && settings.friction >= 0.0F
        && std::isfinite(settings.gravity_factor)
        && std::isfinite(settings.max_slope_angle_radians)
        && settings.max_slope_angle_radians >= 0.0F
        && settings.max_slope_angle_radians <= half_pi;
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

EActivation activation(const BFPhysicsCharacterSettings &settings) {
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

BFPhysicsQuat from_jolt(QuatArg value) {
    return BFPhysicsQuat {
        value.GetX(),
        value.GetY(),
        value.GetZ(),
        value.GetW(),
    };
}

Vec3 to_vec3(BFPhysicsVec3 value) {
    return Vec3(value.x, value.y, value.z);
}

RVec3 to_rvec3(BFPhysicsVec3 value) {
    return RVec3(value.x, value.y, value.z);
}

Quat to_quat(BFPhysicsQuat value) {
    return Quat(value.x, value.y, value.z, value.w);
}

struct ContactRecord {
    BFPhysicsContactEvent event;
    std::vector<BFPhysicsContactPoint> points;
};

bool point_less(const BFPhysicsContactPoint &first, const BFPhysicsContactPoint &second) {
    return std::tie(
        first.position_on1.x,
        first.position_on1.y,
        first.position_on1.z,
        first.position_on2.x,
        first.position_on2.y,
        first.position_on2.z)
        < std::tie(
            second.position_on1.x,
            second.position_on1.y,
            second.position_on1.z,
            second.position_on2.x,
            second.position_on2.y,
            second.position_on2.z);
}

bool contact_less(const ContactRecord &first, const ContactRecord &second) {
    const auto first_key = std::tie(
        first.event.body1_id,
        first.event.body2_id,
        first.event.sub_shape1_id,
        first.event.sub_shape2_id,
        first.event.kind,
        first.event.normal.x,
        first.event.normal.y,
        first.event.normal.z,
        first.event.penetration_depth);
    const auto second_key = std::tie(
        second.event.body1_id,
        second.event.body2_id,
        second.event.sub_shape1_id,
        second.event.sub_shape2_id,
        second.event.kind,
        second.event.normal.x,
        second.event.normal.y,
        second.event.normal.z,
        second.event.penetration_depth);
    if (first_key != second_key) {
        return first_key < second_key;
    }
    return std::lexicographical_compare(
        first.points.begin(),
        first.points.end(),
        second.points.begin(),
        second.points.end(),
        point_less);
}

class ContactRecorder final : public ContactListener {
public:
    void BeginStep() {
        std::lock_guard lock(mutex_);
        records_.clear();
    }

    void FinishStep() {
        std::lock_guard lock(mutex_);
        for (ContactRecord &record : records_) {
            std::sort(record.points.begin(), record.points.end(), point_less);
            record.event.point_count = static_cast<uint32_t>(record.points.size());
        }
        std::sort(records_.begin(), records_.end(), contact_less);
    }

    void OnContactAdded(
        const Body &body1,
        const Body &body2,
        const ContactManifold &manifold,
        ContactSettings &settings) override {
        Record(BF_PHYSICS_CONTACT_ADDED, body1, body2, manifold, settings);
    }

    void OnContactPersisted(
        const Body &body1,
        const Body &body2,
        const ContactManifold &manifold,
        ContactSettings &settings) override {
        Record(BF_PHYSICS_CONTACT_PERSISTED, body1, body2, manifold, settings);
    }

    void OnContactRemoved(const SubShapeIDPair &pair) override {
        ContactRecord record {};
        record.event.kind = BF_PHYSICS_CONTACT_REMOVED;
        record.event.body1_id = pair.GetBody1ID().GetIndexAndSequenceNumber();
        record.event.body2_id = pair.GetBody2ID().GetIndexAndSequenceNumber();
        record.event.sub_shape1_id = pair.GetSubShapeID1().GetValue();
        record.event.sub_shape2_id = pair.GetSubShapeID2().GetValue();
        std::lock_guard lock(mutex_);
        records_.push_back(std::move(record));
    }

    uint32_t Count() const {
        return static_cast<uint32_t>(records_.size());
    }

    const ContactRecord *Get(uint32_t index) const {
        return index < records_.size() ? &records_[index] : nullptr;
    }

private:
    void Record(
        uint32_t kind,
        const Body &body1,
        const Body &body2,
        const ContactManifold &manifold,
        const ContactSettings &settings) {
        ContactRecord record {};
        record.event.kind = kind;
        record.event.body1_id = body1.GetID().GetIndexAndSequenceNumber();
        record.event.body2_id = body2.GetID().GetIndexAndSequenceNumber();
        record.event.sub_shape1_id = manifold.mSubShapeID1.GetValue();
        record.event.sub_shape2_id = manifold.mSubShapeID2.GetValue();
        record.event.normal = from_jolt(manifold.mWorldSpaceNormal);
        record.event.penetration_depth = manifold.mPenetrationDepth;
        record.event.combined_friction = settings.mCombinedFriction;
        record.event.combined_restitution = settings.mCombinedRestitution;
        record.event.is_sensor = settings.mIsSensor ? 1 : 0;
        const uint32_t count = manifold.mRelativeContactPointsOn1.size();
        record.points.reserve(count);
        for (uint32_t index = 0; index < count; ++index) {
            record.points.push_back(BFPhysicsContactPoint {
                from_jolt(manifold.GetWorldSpaceContactPointOn1(index)),
                from_jolt(manifold.GetWorldSpaceContactPointOn2(index)),
            });
        }
        std::lock_guard lock(mutex_);
        records_.push_back(std::move(record));
    }

    mutable std::mutex mutex_;
    std::vector<ContactRecord> records_;
};

int32_t require_body(const BFPhysicsWorld *world, BodyID body_id);
int32_t require_character(
    const BFPhysicsWorld *world,
    uint32_t character_id,
    Character **out_character);

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
        system.SetContactListener(&contact_recorder);
    }

    ~BFPhysicsWorld() {
        system.SetContactListener(nullptr);
        for (const auto &entry : characters) {
            Character *character = entry.second;
            character->RemoveFromPhysicsSystem();
            delete character;
        }
    }

    BroadPhaseLayers broad_phase_layers;
    ObjectVsBroadPhaseFilter object_vs_broad_phase_filter;
    LayerPairFilter object_layer_filter;
    TempAllocatorMalloc temp_allocator;
    JobSystemThreadPool job_system;
    PhysicsSystem system;
    ContactRecorder contact_recorder;
    std::unordered_map<uint32_t, Character *> characters;
};

namespace {

int32_t require_body(const BFPhysicsWorld *world, BodyID body_id) {
    if (!world->system.GetBodyInterface().IsAdded(body_id)) {
        return BF_PHYSICS_STATUS_BODY_NOT_FOUND;
    }
    return BF_PHYSICS_STATUS_OK;
}

int32_t require_character(
    const BFPhysicsWorld *world,
    uint32_t character_id,
    Character **out_character) {
    const auto character = world->characters.find(character_id);
    if (character == world->characters.end()) {
        return BF_PHYSICS_STATUS_CHARACTER_NOT_FOUND;
    }
    *out_character = character->second;
    return BF_PHYSICS_STATUS_OK;
}

uint32_t ground_state(CharacterBase::EGroundState state) {
    switch (state) {
    case CharacterBase::EGroundState::OnGround:
        return BF_PHYSICS_GROUND_ON_GROUND;
    case CharacterBase::EGroundState::OnSteepGround:
        return BF_PHYSICS_GROUND_ON_STEEP_GROUND;
    case CharacterBase::EGroundState::NotSupported:
        return BF_PHYSICS_GROUND_NOT_SUPPORTED;
    case CharacterBase::EGroundState::InAir:
        return BF_PHYSICS_GROUND_IN_AIR;
    }
    return BF_PHYSICS_GROUND_IN_AIR;
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

extern "C" int32_t bf_physics_world_create_capsule_body(
    BFPhysicsWorld *world,
    const BFPhysicsBodySettings *settings,
    float half_height,
    float radius,
    uint32_t *out_body_id) {
    if (!std::isfinite(half_height) || !std::isfinite(radius) || half_height <= 0.0F
        || radius <= 0.0F) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    return create_body(
        world,
        settings,
        new CapsuleShape(half_height, radius),
        out_body_id);
}

extern "C" int32_t bf_physics_world_destroy_body(BFPhysicsWorld *world, uint32_t body_id) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (world->characters.find(body_id) != world->characters.end()) {
        return BF_PHYSICS_STATUS_BODY_OWNED_BY_CHARACTER;
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

extern "C" int32_t bf_physics_world_body_rotation(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsQuat *out_rotation) {
    if (world == nullptr || out_rotation == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    *out_rotation = from_jolt(world->system.GetBodyInterface().GetRotation(id));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_set_body_rotation(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsQuat rotation) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!finite(rotation) || !to_quat(rotation).IsNormalized()) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    world->system.GetBodyInterface().SetRotation(id, to_quat(rotation), EActivation::Activate);
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
        to_vec3(velocity));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_body_angular_velocity(
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
    *out_velocity = from_jolt(world->system.GetBodyInterface().GetAngularVelocity(id));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_set_body_angular_velocity(
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
    world->system.GetBodyInterface().SetAngularVelocity(id, to_vec3(velocity));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_add_body_force(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 force) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!finite(force)) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    world->system.GetBodyInterface().AddForce(id, to_vec3(force));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_add_body_force_at_point(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 force,
    BFPhysicsVec3 point) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!finite(force) || !finite(point)) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    world->system.GetBodyInterface().AddForce(id, to_vec3(force), to_rvec3(point));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_add_body_torque(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 torque) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!finite(torque)) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    world->system.GetBodyInterface().AddTorque(id, to_vec3(torque));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_add_body_impulse(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 impulse) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!finite(impulse)) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    world->system.GetBodyInterface().AddImpulse(id, to_vec3(impulse));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_add_body_impulse_at_point(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 impulse,
    BFPhysicsVec3 point) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!finite(impulse) || !finite(point)) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    world->system.GetBodyInterface().AddImpulse(id, to_vec3(impulse), to_rvec3(point));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_add_body_angular_impulse(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 impulse) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!finite(impulse)) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    const BodyID id(body_id);
    const int32_t status = require_body(world, id);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    world->system.GetBodyInterface().AddAngularImpulse(id, to_vec3(impulse));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_create_character(
    BFPhysicsWorld *world,
    const BFPhysicsCharacterSettings *settings,
    uint32_t *out_character_id) {
    if (world == nullptr || settings == nullptr || out_character_id == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!valid_character_settings(*settings)) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }

    JPH::CharacterSettings character_settings;
    const Vec3 shape_offset(
        0.0F,
        settings->capsule_half_height + settings->capsule_radius,
        0.0F);
    character_settings.mShape = new RotatedTranslatedShape(
        shape_offset,
        Quat::sIdentity(),
        new CapsuleShape(settings->capsule_half_height, settings->capsule_radius));
    character_settings.mSupportingVolume =
        Plane(Vec3::sAxisY(), -settings->capsule_radius);
    character_settings.mLayer = kMovingLayer;
    character_settings.mMass = settings->mass;
    character_settings.mFriction = settings->friction;
    character_settings.mGravityFactor = settings->gravity_factor;
    character_settings.mMaxSlopeAngle = settings->max_slope_angle_radians;
    Character *character = new Character(
        &character_settings,
        to_rvec3(settings->position),
        to_quat(settings->rotation),
        0,
        &world->system);
    const BodyID body_id = character->GetBodyID();
    if (body_id.IsInvalid()) {
        delete character;
        return BF_PHYSICS_STATUS_BODY_CAPACITY_EXHAUSTED;
    }

    character->AddToPhysicsSystem(activation(*settings));
    const uint32_t raw_id = body_id.GetIndexAndSequenceNumber();
    world->characters.emplace(raw_id, character);
    *out_character_id = raw_id;
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_destroy_character(
    BFPhysicsWorld *world,
    uint32_t character_id) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    Character *character = nullptr;
    const int32_t status = require_character(world, character_id, &character);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    character->RemoveFromPhysicsSystem();
    world->characters.erase(character_id);
    delete character;
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_set_character_linear_velocity(
    BFPhysicsWorld *world,
    uint32_t character_id,
    BFPhysicsVec3 velocity) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!finite(velocity)) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    Character *character = nullptr;
    const int32_t status = require_character(world, character_id, &character);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    character->SetLinearVelocity(to_vec3(velocity));
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_refresh_character_ground_state(
    BFPhysicsWorld *world,
    uint32_t character_id,
    float max_separation_distance) {
    if (world == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    if (!std::isfinite(max_separation_distance) || max_separation_distance < 0.0F) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    Character *character = nullptr;
    const int32_t status = require_character(world, character_id, &character);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    character->PostSimulation(max_separation_distance);
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_character_state(
    const BFPhysicsWorld *world,
    uint32_t character_id,
    BFPhysicsCharacterState *out_state) {
    if (world == nullptr || out_state == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    Character *character = nullptr;
    const int32_t status = require_character(world, character_id, &character);
    if (status != BF_PHYSICS_STATUS_OK) {
        return status;
    }
    const BodyID ground_body = character->GetGroundBodyID();
    *out_state = BFPhysicsCharacterState {
        character->GetBodyID().GetIndexAndSequenceNumber(),
        from_jolt(character->GetPosition()),
        from_jolt(character->GetRotation()),
        from_jolt(character->GetLinearVelocity()),
        ground_state(character->GetGroundState()),
        ground_body.GetIndexAndSequenceNumber(),
        character->GetGroundSubShapeID().GetValue(),
        from_jolt(character->GetGroundPosition()),
        from_jolt(character->GetGroundNormal()),
        from_jolt(character->GetGroundVelocity()),
    };
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_contact_event_count(
    const BFPhysicsWorld *world,
    uint32_t *out_count) {
    if (world == nullptr || out_count == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    *out_count = world->contact_recorder.Count();
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_contact_event(
    const BFPhysicsWorld *world,
    uint32_t event_index,
    BFPhysicsContactEvent *out_event) {
    if (world == nullptr || out_event == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    const ContactRecord *record = world->contact_recorder.Get(event_index);
    if (record == nullptr) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    *out_event = record->event;
    return BF_PHYSICS_STATUS_OK;
}

extern "C" int32_t bf_physics_world_contact_point(
    const BFPhysicsWorld *world,
    uint32_t event_index,
    uint32_t point_index,
    BFPhysicsContactPoint *out_point) {
    if (world == nullptr || out_point == nullptr) {
        return BF_PHYSICS_STATUS_NULL_POINTER;
    }
    const ContactRecord *record = world->contact_recorder.Get(event_index);
    if (record == nullptr || point_index >= record->points.size()) {
        return BF_PHYSICS_STATUS_INVALID_ARGUMENT;
    }
    *out_point = record->points[point_index];
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
    world->contact_recorder.BeginStep();
    *out_update_errors = static_cast<uint32_t>(world->system.Update(
        delta_seconds,
        collision_steps,
        &world->temp_allocator,
        &world->job_system));
    world->contact_recorder.FinishStep();
    return BF_PHYSICS_STATUS_OK;
}
