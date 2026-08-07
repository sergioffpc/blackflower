#ifndef BLACKFLOWER_PHYSICS_WRAPPER_H
#define BLACKFLOWER_PHYSICS_WRAPPER_H

#include <stdint.h>

#ifdef __cplusplus
#define BF_PHYSICS_NOEXCEPT noexcept
extern "C" {
#else
#define BF_PHYSICS_NOEXCEPT
#endif

#define BF_PHYSICS_STATUS_OK 0
#define BF_PHYSICS_STATUS_NULL_POINTER 1
#define BF_PHYSICS_STATUS_INVALID_ARGUMENT 2
#define BF_PHYSICS_STATUS_INITIALIZATION_FAILED 3
#define BF_PHYSICS_STATUS_BODY_CAPACITY_EXHAUSTED 4
#define BF_PHYSICS_STATUS_BODY_NOT_FOUND 5
#define BF_PHYSICS_STATUS_CHARACTER_NOT_FOUND 6
#define BF_PHYSICS_STATUS_BODY_OWNED_BY_CHARACTER 7
#define BF_PHYSICS_STATUS_SHAPE_CREATION_FAILED 8
#define BF_PHYSICS_STATUS_OUT_OF_MEMORY 9
#define BF_PHYSICS_STATUS_NATIVE_FAILURE 10
#define BF_PHYSICS_STATUS_CONFIGURATION_MISMATCH 11

#define BF_PHYSICS_MAX_CONVEX_HULL_POINTS 256

#define BF_PHYSICS_MOTION_STATIC 0
#define BF_PHYSICS_MOTION_KINEMATIC 1
#define BF_PHYSICS_MOTION_DYNAMIC 2

#define BF_PHYSICS_CONTACT_ADDED 0
#define BF_PHYSICS_CONTACT_PERSISTED 1
#define BF_PHYSICS_CONTACT_REMOVED 2

#define BF_PHYSICS_GROUND_ON_GROUND 0
#define BF_PHYSICS_GROUND_ON_STEEP_GROUND 1
#define BF_PHYSICS_GROUND_NOT_SUPPORTED 2
#define BF_PHYSICS_GROUND_IN_AIR 3

#define BF_PHYSICS_UPDATE_MANIFOLD_CACHE_FULL (1u << 0)
#define BF_PHYSICS_UPDATE_BODY_PAIR_CACHE_FULL (1u << 1)
#define BF_PHYSICS_UPDATE_CONTACT_CONSTRAINTS_FULL (1u << 2)

typedef struct BFPhysicsWorld BFPhysicsWorld;
typedef struct BFPhysicsShape BFPhysicsShape;

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

typedef struct BFPhysicsTriangle {
    uint32_t first;
    uint32_t second;
    uint32_t third;
} BFPhysicsTriangle;

typedef struct BFPhysicsCompoundChild {
    const BFPhysicsShape *shape;
    BFPhysicsVec3 position;
    BFPhysicsQuat rotation;
} BFPhysicsCompoundChild;

typedef struct BFPhysicsCharacterSettings {
    BFPhysicsVec3 position;
    BFPhysicsQuat rotation;
    float capsule_half_height;
    float capsule_radius;
    float mass;
    float friction;
    float gravity_factor;
    float max_slope_angle_radians;
    uint8_t active;
} BFPhysicsCharacterSettings;

typedef struct BFPhysicsCharacterState {
    uint32_t body_id;
    BFPhysicsVec3 position;
    BFPhysicsQuat rotation;
    BFPhysicsVec3 linear_velocity;
    uint32_t ground_state;
    uint32_t ground_body_id;
    uint32_t ground_sub_shape_id;
    BFPhysicsVec3 ground_position;
    BFPhysicsVec3 ground_normal;
    BFPhysicsVec3 ground_velocity;
} BFPhysicsCharacterState;

typedef struct BFPhysicsContactEvent {
    uint32_t kind;
    uint32_t body1_id;
    uint32_t body2_id;
    uint32_t sub_shape1_id;
    uint32_t sub_shape2_id;
    BFPhysicsVec3 normal;
    float penetration_depth;
    float combined_friction;
    float combined_restitution;
    uint8_t is_sensor;
    uint32_t point_count;
} BFPhysicsContactEvent;

typedef struct BFPhysicsContactPoint {
    BFPhysicsVec3 position_on1;
    BFPhysicsVec3 position_on2;
} BFPhysicsContactPoint;

typedef struct BFPhysicsRayHit {
    uint32_t body_id;
    uint32_t sub_shape_id;
    float fraction;
    BFPhysicsVec3 position;
    BFPhysicsVec3 normal;
} BFPhysicsRayHit;

BFPhysicsVersion bf_physics_jolt_version(void) BF_PHYSICS_NOEXCEPT;

int32_t bf_physics_shape_create_sphere(
    float radius,
    BFPhysicsShape **out_shape) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_shape_create_box(
    BFPhysicsVec3 half_extent,
    BFPhysicsShape **out_shape) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_shape_create_capsule(
    float half_height,
    float radius,
    BFPhysicsShape **out_shape) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_shape_create_convex_hull(
    const BFPhysicsVec3 *points,
    uint32_t point_count,
    BFPhysicsShape **out_shape) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_shape_create_compound(
    const BFPhysicsCompoundChild *children,
    uint32_t child_count,
    BFPhysicsShape **out_shape) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_shape_create_triangle_mesh(
    const BFPhysicsVec3 *vertices,
    uint32_t vertex_count,
    const BFPhysicsTriangle *triangles,
    uint32_t triangle_count,
    BFPhysicsShape **out_shape) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_shape_destroy(BFPhysicsShape *shape) BF_PHYSICS_NOEXCEPT;

int32_t bf_physics_world_create(
    const BFPhysicsWorldConfig *config,
    BFPhysicsWorld **out_world) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_destroy(BFPhysicsWorld *world) BF_PHYSICS_NOEXCEPT;

int32_t bf_physics_world_create_body(
    BFPhysicsWorld *world,
    const BFPhysicsBodySettings *settings,
    const BFPhysicsShape *shape,
    uint32_t *out_body_id) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_destroy_body(
    BFPhysicsWorld *world,
    uint32_t body_id) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_body_exists(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    uint8_t *out_exists) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_body_is_active(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    uint8_t *out_active) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_body_position(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 *out_position) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_body_rotation(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsQuat *out_rotation) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_set_body_rotation(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsQuat rotation) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_body_linear_velocity(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 *out_velocity) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_set_body_linear_velocity(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 velocity) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_body_angular_velocity(
    const BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 *out_velocity) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_set_body_angular_velocity(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 velocity) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_add_body_force(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 force) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_add_body_force_at_point(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 force,
    BFPhysicsVec3 point) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_add_body_torque(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 torque) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_add_body_impulse(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 impulse) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_add_body_impulse_at_point(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 impulse,
    BFPhysicsVec3 point) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_add_body_angular_impulse(
    BFPhysicsWorld *world,
    uint32_t body_id,
    BFPhysicsVec3 impulse) BF_PHYSICS_NOEXCEPT;

int32_t bf_physics_world_create_character(
    BFPhysicsWorld *world,
    const BFPhysicsCharacterSettings *settings,
    uint32_t *out_character_id) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_destroy_character(
    BFPhysicsWorld *world,
    uint32_t character_id) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_set_character_linear_velocity(
    BFPhysicsWorld *world,
    uint32_t character_id,
    BFPhysicsVec3 velocity) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_refresh_character_ground_state(
    BFPhysicsWorld *world,
    uint32_t character_id,
    float max_separation_distance) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_character_state(
    const BFPhysicsWorld *world,
    uint32_t character_id,
    BFPhysicsCharacterState *out_state) BF_PHYSICS_NOEXCEPT;

int32_t bf_physics_world_contact_event_count(
    const BFPhysicsWorld *world,
    uint32_t *out_count) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_contact_event(
    const BFPhysicsWorld *world,
    uint32_t event_index,
    BFPhysicsContactEvent *out_event) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_contact_point(
    const BFPhysicsWorld *world,
    uint32_t event_index,
    uint32_t point_index,
    BFPhysicsContactPoint *out_point) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_cast_ray(
    const BFPhysicsWorld *world,
    BFPhysicsVec3 origin,
    BFPhysicsVec3 displacement,
    uint8_t *out_has_hit,
    BFPhysicsRayHit *out_hit) BF_PHYSICS_NOEXCEPT;

int32_t bf_physics_world_optimize_broad_phase(
    BFPhysicsWorld *world) BF_PHYSICS_NOEXCEPT;
int32_t bf_physics_world_update(
    BFPhysicsWorld *world,
    float delta_seconds,
    int32_t collision_steps,
    uint32_t *out_update_errors) BF_PHYSICS_NOEXCEPT;

#ifdef __cplusplus
}
#endif

#undef BF_PHYSICS_NOEXCEPT

#endif
