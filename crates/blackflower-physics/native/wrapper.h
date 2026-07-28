#ifndef BLACKFLOWER_PHYSICS_WRAPPER_H
#define BLACKFLOWER_PHYSICS_WRAPPER_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BF_PHYSICS_STATUS_OK 0
#define BF_PHYSICS_STATUS_NULL_POINTER 1
#define BF_PHYSICS_STATUS_INVALID_ARGUMENT 2
#define BF_PHYSICS_STATUS_INITIALIZATION_FAILED 3
#define BF_PHYSICS_STATUS_BODY_CAPACITY_EXHAUSTED 4
#define BF_PHYSICS_STATUS_BODY_NOT_FOUND 5

#define BF_PHYSICS_MOTION_STATIC 0
#define BF_PHYSICS_MOTION_KINEMATIC 1
#define BF_PHYSICS_MOTION_DYNAMIC 2

#define BF_PHYSICS_UPDATE_MANIFOLD_CACHE_FULL (1u << 0)
#define BF_PHYSICS_UPDATE_BODY_PAIR_CACHE_FULL (1u << 1)
#define BF_PHYSICS_UPDATE_CONTACT_CONSTRAINTS_FULL (1u << 2)

typedef struct BFPhysicsWorld BFPhysicsWorld;

typedef struct BFPhysicsVec3 {
    float x;
    float y;
    float z;
} BFPhysicsVec3;

typedef struct BFPhysicsQuat {
    float x;
    float y;
    float z;
    float w;
} BFPhysicsQuat;

typedef struct BFPhysicsVersion {
    uint32_t major;
    uint32_t minor;
    uint32_t patch;
} BFPhysicsVersion;

typedef struct BFPhysicsWorldConfig {
    uint32_t max_bodies;
    uint32_t body_mutexes;
    uint32_t max_body_pairs;
    uint32_t max_contact_constraints;
    int32_t worker_threads;
} BFPhysicsWorldConfig;

typedef struct BFPhysicsBodySettings {
    BFPhysicsVec3 position;
    BFPhysicsQuat rotation;
    uint32_t motion_type;
    uint8_t active;
} BFPhysicsBodySettings;

BFPhysicsVersion bf_physics_jolt_version(void);

int32_t bf_physics_world_create(
    const BFPhysicsWorldConfig *config,
    BFPhysicsWorld **out_world);
void bf_physics_world_destroy(BFPhysicsWorld *world);

int32_t bf_physics_world_create_sphere_body(
    BFPhysicsWorld *world,
    const BFPhysicsBodySettings *settings,
    float radius,
    uint32_t *out_body_id);
int32_t bf_physics_world_create_box_body(
    BFPhysicsWorld *world,
    const BFPhysicsBodySettings *settings,
    BFPhysicsVec3 half_extent,
    uint32_t *out_body_id);
int32_t bf_physics_world_destroy_body(BFPhysicsWorld *world, uint32_t body_id);
int32_t bf_physics_world_body_exists(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    uint8_t *out_exists);
int32_t bf_physics_world_body_is_active(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    uint8_t *out_active);
int32_t bf_physics_world_body_position(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 *out_position);
int32_t bf_physics_world_body_linear_velocity(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 *out_velocity);
int32_t bf_physics_world_set_body_linear_velocity(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 velocity);

void bf_physics_world_optimize_broad_phase(BFPhysicsWorld *world);
int32_t bf_physics_world_update(
    BFPhysicsWorld *world,
    float delta_seconds,
    int32_t collision_steps,
    uint32_t *out_update_errors);

#ifdef __cplusplus
}
#endif

#endif
