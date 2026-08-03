#ifndef BLACKFLOWER_DESTRUCTION_WRAPPER_H
#define BLACKFLOWER_DESTRUCTION_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BF_DESTRUCTION_STATUS_OK 0
#define BF_DESTRUCTION_STATUS_NULL_POINTER 1
#define BF_DESTRUCTION_STATUS_INVALID_ARGUMENT 2
#define BF_DESTRUCTION_STATUS_ALLOCATION_FAILED 3
#define BF_DESTRUCTION_STATUS_ASSET_CREATION_FAILED 4
#define BF_DESTRUCTION_STATUS_FAMILY_CREATION_FAILED 5
#define BF_DESTRUCTION_STATUS_ACTOR_NOT_FOUND 6
#define BF_DESTRUCTION_STATUS_CAPACITY_EXCEEDED 7
#define BF_DESTRUCTION_STATUS_STRESS_UNAVAILABLE 8
#define BF_DESTRUCTION_STATUS_STRESS_FAILED 9

#define BF_DESTRUCTION_FRACTURE_BOND 0
#define BF_DESTRUCTION_FRACTURE_CHUNK 1

#define BF_DESTRUCTION_FORCE 0
#define BF_DESTRUCTION_ACCELERATION 1

#define BF_DESTRUCTION_INVALID_INDEX 0xFFFFFFFFU

typedef struct BFDestructionAsset BFDestructionAsset;
typedef struct BFDestructionFamily BFDestructionFamily;

typedef struct BFDestructionVec3 {
    float x;
    float y;
    float z;
} BFDestructionVec3;

typedef struct BFDestructionChunkDesc {
    BFDestructionVec3 centroid;
    float volume;
    uint32_t parent_chunk_index;
    uint32_t user_data;
    uint8_t support;
} BFDestructionChunkDesc;

typedef struct BFDestructionBondDesc {
    BFDestructionVec3 normal;
    float area;
    BFDestructionVec3 centroid;
    uint32_t user_data;
    uint32_t chunk_index0;
    uint32_t chunk_index1;
} BFDestructionBondDesc;

typedef struct BFDestructionFractureData {
    uint32_t kind;
    uint32_t user_data;
    uint32_t index0;
    uint32_t index1;
    float health;
} BFDestructionFractureData;

typedef struct BFDestructionStressSettings {
    uint32_t max_solver_iterations_per_frame;
    uint32_t graph_reduction_level;
    float compression_elastic_limit;
    float compression_fatal_limit;
    float tension_elastic_limit;
    float tension_fatal_limit;
    float shear_elastic_limit;
    float shear_fatal_limit;
} BFDestructionStressSettings;

typedef struct BFDestructionStressStats {
    uint32_t frame_count;
    uint32_t bond_count;
    uint32_t overstressed_bond_count;
    float linear_error;
    float angular_error;
    uint8_t converged;
} BFDestructionStressStats;

const char* bf_destruction_blast_version(void);
uint8_t bf_destruction_stress_supported(void);

int32_t bf_destruction_asset_create(
    const BFDestructionChunkDesc* chunks,
    uint32_t chunk_count,
    const BFDestructionBondDesc* bonds,
    uint32_t bond_count,
    BFDestructionAsset** out_asset);
void bf_destruction_asset_destroy(BFDestructionAsset* asset);
uint32_t bf_destruction_asset_chunk_count(const BFDestructionAsset* asset);
uint32_t bf_destruction_asset_bond_count(const BFDestructionAsset* asset);
uint32_t bf_destruction_asset_support_chunk_count(const BFDestructionAsset* asset);
uint32_t bf_destruction_asset_graph_node_count(const BFDestructionAsset* asset);

int32_t bf_destruction_family_create(
    const BFDestructionAsset* asset,
    float initial_bond_health,
    float initial_chunk_health,
    BFDestructionFamily** out_family);
void bf_destruction_family_destroy(BFDestructionFamily* family);
uint32_t bf_destruction_family_actor_count(const BFDestructionFamily* family);
int32_t bf_destruction_family_actor_ids(
    const BFDestructionFamily* family,
    uint32_t* actor_ids,
    uint32_t capacity,
    uint32_t* out_count);
int32_t bf_destruction_family_visible_chunks(
    const BFDestructionFamily* family,
    uint32_t actor_id,
    uint32_t* chunk_indices,
    uint32_t capacity,
    uint32_t* out_count);
int32_t bf_destruction_family_apply_fracture(
    BFDestructionFamily* family,
    uint32_t actor_id,
    const BFDestructionFractureData* commands,
    uint32_t command_count,
    BFDestructionFractureData* events,
    uint32_t event_capacity,
    uint32_t* out_event_count);
int32_t bf_destruction_family_split_actor(
    BFDestructionFamily* family,
    uint32_t actor_id,
    uint32_t* new_actor_ids,
    uint32_t capacity,
    uint32_t* out_count);

int32_t bf_destruction_family_enable_stress(
    BFDestructionFamily* family,
    const BFDestructionStressSettings* settings,
    float density);
int32_t bf_destruction_family_stress_add_force(
    BFDestructionFamily* family,
    uint32_t graph_node_index,
    BFDestructionVec3 force,
    uint32_t mode);
int32_t bf_destruction_family_stress_update(BFDestructionFamily* family);
int32_t bf_destruction_family_apply_stress(
    BFDestructionFamily* family,
    uint32_t actor_id,
    BFDestructionFractureData* events,
    uint32_t event_capacity,
    uint32_t* out_event_count);
int32_t bf_destruction_family_stress_stats(
    const BFDestructionFamily* family,
    BFDestructionStressStats* out_stats);

#ifdef __cplusplus
}
#endif

#endif
