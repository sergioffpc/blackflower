#include "wrapper.h"

#include "NvBlast.h"
#if BLACKFLOWER_BLAST_HAS_STRESS
#include "NvBlastExtStressSolver.h"
#endif

#include <new>

struct BFDestructionAsset {
    void* memory;
    NvBlastAsset* blast;
};

struct BFDestructionFamily {
    void* memory;
    NvBlastFamily* blast;
    const BFDestructionAsset* asset;
#if BLACKFLOWER_BLAST_HAS_STRESS
    Nv::Blast::ExtStressSolver* stress;
#endif
};

namespace {

constexpr size_t kBlastAlignment = 16u;

void* allocate_aligned(size_t size) {
    return ::operator new(size, std::align_val_t(kBlastAlignment), std::nothrow);
}

void free_aligned(void* memory) {
    ::operator delete(memory, std::align_val_t(kBlastAlignment));
}

NvBlastActor* actor_from_id(const BFDestructionFamily* family, uint32_t actor_id) {
    if (family == nullptr || family->blast == nullptr) {
        return nullptr;
    }
    return NvBlastFamilyGetActorByIndex(family->blast, actor_id, nullptr);
}

uint32_t maximum_event_count(const BFDestructionFamily* family) {
    return NvBlastAssetGetChunkCount(family->asset->blast, nullptr) +
        NvBlastAssetGetBondCount(family->asset->blast, nullptr);
}

int32_t write_events(
    const NvBlastFractureBuffers& native,
    BFDestructionFractureData* events,
    uint32_t capacity,
    uint32_t* out_count) {
    const uint64_t total = static_cast<uint64_t>(native.bondFractureCount) +
        static_cast<uint64_t>(native.chunkFractureCount);
    if (total > capacity) {
        return BF_DESTRUCTION_STATUS_CAPACITY_EXCEEDED;
    }
    uint32_t cursor = 0u;
    for (uint32_t index = 0u; index < native.bondFractureCount; ++index) {
        const NvBlastBondFractureData& event = native.bondFractures[index];
        events[cursor++] = BFDestructionFractureData{
            BF_DESTRUCTION_FRACTURE_BOND,
            event.userdata,
            event.nodeIndex0,
            event.nodeIndex1,
            event.health,
        };
    }
    for (uint32_t index = 0u; index < native.chunkFractureCount; ++index) {
        const NvBlastChunkFractureData& event = native.chunkFractures[index];
        events[cursor++] = BFDestructionFractureData{
            BF_DESTRUCTION_FRACTURE_CHUNK,
            event.userdata,
            event.chunkIndex,
            BF_DESTRUCTION_INVALID_INDEX,
            event.health,
        };
    }
    *out_count = cursor;
    return BF_DESTRUCTION_STATUS_OK;
}

int32_t apply_native_fracture(
    BFDestructionFamily* family,
    NvBlastActor* actor,
    const NvBlastFractureBuffers& commands,
    BFDestructionFractureData* events,
    uint32_t event_capacity,
    uint32_t* out_event_count) {
    const uint32_t required_capacity = maximum_event_count(family);
    if (event_capacity < required_capacity) {
        return BF_DESTRUCTION_STATUS_CAPACITY_EXCEEDED;
    }
    NvBlastBondFractureData* bond_events = required_capacity == 0u
        ? nullptr
        : new (std::nothrow) NvBlastBondFractureData[required_capacity];
    NvBlastChunkFractureData* chunk_events = required_capacity == 0u
        ? nullptr
        : new (std::nothrow) NvBlastChunkFractureData[required_capacity];
    if (required_capacity != 0u && (bond_events == nullptr || chunk_events == nullptr)) {
        delete[] bond_events;
        delete[] chunk_events;
        return BF_DESTRUCTION_STATUS_ALLOCATION_FAILED;
    }
    NvBlastFractureBuffers event_buffers{
        required_capacity,
        required_capacity,
        bond_events,
        chunk_events,
    };
    NvBlastActorApplyFracture(&event_buffers, actor, &commands, nullptr, nullptr);
    const int32_t status = write_events(event_buffers, events, event_capacity, out_event_count);
    delete[] bond_events;
    delete[] chunk_events;
    return status;
}

} // namespace

extern "C" {

const char* bf_destruction_blast_version(void) {
    return "5.0.6";
}

uint8_t bf_destruction_stress_supported(void) {
#if BLACKFLOWER_BLAST_HAS_STRESS
    return 1u;
#else
    return 0u;
#endif
}

int32_t bf_destruction_asset_create(
    const BFDestructionChunkDesc* chunks,
    uint32_t chunk_count,
    const BFDestructionBondDesc* bonds,
    uint32_t bond_count,
    BFDestructionAsset** out_asset) {
    if (out_asset == nullptr || chunks == nullptr) {
        return BF_DESTRUCTION_STATUS_NULL_POINTER;
    }
    *out_asset = nullptr;
    if (chunk_count == 0u || (bond_count != 0u && bonds == nullptr)) {
        return BF_DESTRUCTION_STATUS_INVALID_ARGUMENT;
    }

    NvBlastChunkDesc* native_chunks = new (std::nothrow) NvBlastChunkDesc[chunk_count];
    NvBlastBondDesc* native_bonds = bond_count == 0u
        ? nullptr
        : new (std::nothrow) NvBlastBondDesc[bond_count];
    if (native_chunks == nullptr || (bond_count != 0u && native_bonds == nullptr)) {
        delete[] native_chunks;
        delete[] native_bonds;
        return BF_DESTRUCTION_STATUS_ALLOCATION_FAILED;
    }

    for (uint32_t index = 0u; index < chunk_count; ++index) {
        const BFDestructionChunkDesc& source = chunks[index];
        NvBlastChunkDesc& target = native_chunks[index];
        target.centroid[0] = source.centroid.x;
        target.centroid[1] = source.centroid.y;
        target.centroid[2] = source.centroid.z;
        target.volume = source.volume;
        target.parentChunkDescIndex = source.parent_chunk_index;
        target.flags = source.support != 0u ? NvBlastChunkDesc::SupportFlag : NvBlastChunkDesc::NoFlags;
        target.userData = source.user_data;
    }
    for (uint32_t index = 0u; index < bond_count; ++index) {
        const BFDestructionBondDesc& source = bonds[index];
        NvBlastBondDesc& target = native_bonds[index];
        target.bond.normal[0] = source.normal.x;
        target.bond.normal[1] = source.normal.y;
        target.bond.normal[2] = source.normal.z;
        target.bond.area = source.area;
        target.bond.centroid[0] = source.centroid.x;
        target.bond.centroid[1] = source.centroid.y;
        target.bond.centroid[2] = source.centroid.z;
        target.bond.userData = source.user_data;
        target.chunkIndices[0] = source.chunk_index0;
        target.chunkIndices[1] = source.chunk_index1;
    }

    const NvBlastAssetDesc descriptor{chunk_count, native_chunks, bond_count, native_bonds};
    const size_t asset_size = NvBlastGetAssetMemorySize(&descriptor, nullptr);
    const size_t scratch_size = NvBlastGetRequiredScratchForCreateAsset(&descriptor, nullptr);
    void* asset_memory = asset_size == 0u ? nullptr : allocate_aligned(asset_size);
    void* scratch = scratch_size == 0u ? nullptr : allocate_aligned(scratch_size);
    if (asset_memory == nullptr || (scratch_size != 0u && scratch == nullptr)) {
        free_aligned(asset_memory);
        free_aligned(scratch);
        delete[] native_chunks;
        delete[] native_bonds;
        return asset_size == 0u
            ? BF_DESTRUCTION_STATUS_INVALID_ARGUMENT
            : BF_DESTRUCTION_STATUS_ALLOCATION_FAILED;
    }
    NvBlastAsset* blast_asset = NvBlastCreateAsset(asset_memory, &descriptor, scratch, nullptr);
    free_aligned(scratch);
    delete[] native_chunks;
    delete[] native_bonds;
    if (blast_asset == nullptr) {
        free_aligned(asset_memory);
        return BF_DESTRUCTION_STATUS_ASSET_CREATION_FAILED;
    }
    BFDestructionAsset* asset = new (std::nothrow) BFDestructionAsset{asset_memory, blast_asset};
    if (asset == nullptr) {
        free_aligned(asset_memory);
        return BF_DESTRUCTION_STATUS_ALLOCATION_FAILED;
    }
    *out_asset = asset;
    return BF_DESTRUCTION_STATUS_OK;
}

void bf_destruction_asset_destroy(BFDestructionAsset* asset) {
    if (asset == nullptr) {
        return;
    }
    free_aligned(asset->memory);
    delete asset;
}

uint32_t bf_destruction_asset_chunk_count(const BFDestructionAsset* asset) {
    return asset == nullptr ? 0u : NvBlastAssetGetChunkCount(asset->blast, nullptr);
}

uint32_t bf_destruction_asset_bond_count(const BFDestructionAsset* asset) {
    return asset == nullptr ? 0u : NvBlastAssetGetBondCount(asset->blast, nullptr);
}

uint32_t bf_destruction_asset_support_chunk_count(const BFDestructionAsset* asset) {
    return asset == nullptr ? 0u : NvBlastAssetGetSupportChunkCount(asset->blast, nullptr);
}

uint32_t bf_destruction_asset_graph_node_count(const BFDestructionAsset* asset) {
    return asset == nullptr
        ? 0u
        : NvBlastAssetGetSupportGraph(asset->blast, nullptr).nodeCount;
}

int32_t bf_destruction_family_create(
    const BFDestructionAsset* asset,
    float initial_bond_health,
    float initial_chunk_health,
    BFDestructionFamily** out_family) {
    if (asset == nullptr || out_family == nullptr) {
        return BF_DESTRUCTION_STATUS_NULL_POINTER;
    }
    *out_family = nullptr;
    if (!(initial_bond_health > 0.0f) || !(initial_chunk_health > 0.0f)) {
        return BF_DESTRUCTION_STATUS_INVALID_ARGUMENT;
    }
    const size_t family_size = NvBlastAssetGetFamilyMemorySize(asset->blast, nullptr);
    void* family_memory = family_size == 0u ? nullptr : allocate_aligned(family_size);
    if (family_memory == nullptr) {
        return BF_DESTRUCTION_STATUS_ALLOCATION_FAILED;
    }
    NvBlastFamily* blast_family = NvBlastAssetCreateFamily(family_memory, asset->blast, nullptr);
    if (blast_family == nullptr) {
        free_aligned(family_memory);
        return BF_DESTRUCTION_STATUS_FAMILY_CREATION_FAILED;
    }
    const size_t scratch_size = NvBlastFamilyGetRequiredScratchForCreateFirstActor(blast_family, nullptr);
    void* scratch = scratch_size == 0u ? nullptr : allocate_aligned(scratch_size);
    if (scratch_size != 0u && scratch == nullptr) {
        free_aligned(family_memory);
        return BF_DESTRUCTION_STATUS_ALLOCATION_FAILED;
    }
    const NvBlastActorDesc actor_desc{
        initial_bond_health,
        nullptr,
        initial_chunk_health,
        nullptr,
    };
    NvBlastActor* actor = NvBlastFamilyCreateFirstActor(blast_family, &actor_desc, scratch, nullptr);
    free_aligned(scratch);
    if (actor == nullptr) {
        free_aligned(family_memory);
        return BF_DESTRUCTION_STATUS_FAMILY_CREATION_FAILED;
    }
    BFDestructionFamily* family = new (std::nothrow) BFDestructionFamily{
        family_memory,
        blast_family,
        asset,
#if BLACKFLOWER_BLAST_HAS_STRESS
        nullptr,
#endif
    };
    if (family == nullptr) {
        free_aligned(family_memory);
        return BF_DESTRUCTION_STATUS_ALLOCATION_FAILED;
    }
    *out_family = family;
    return BF_DESTRUCTION_STATUS_OK;
}

void bf_destruction_family_destroy(BFDestructionFamily* family) {
    if (family == nullptr) {
        return;
    }
#if BLACKFLOWER_BLAST_HAS_STRESS
    if (family->stress != nullptr) {
        family->stress->release();
    }
#endif
    free_aligned(family->memory);
    delete family;
}

uint32_t bf_destruction_family_actor_count(const BFDestructionFamily* family) {
    return family == nullptr ? 0u : NvBlastFamilyGetActorCount(family->blast, nullptr);
}

int32_t bf_destruction_family_actor_ids(
    const BFDestructionFamily* family,
    uint32_t* actor_ids,
    uint32_t capacity,
    uint32_t* out_count) {
    if (family == nullptr || out_count == nullptr || (capacity != 0u && actor_ids == nullptr)) {
        return BF_DESTRUCTION_STATUS_NULL_POINTER;
    }
    const uint32_t count = NvBlastFamilyGetActorCount(family->blast, nullptr);
    if (capacity < count) {
        return BF_DESTRUCTION_STATUS_CAPACITY_EXCEEDED;
    }
    uint32_t cursor = 0u;
    const uint32_t maximum = NvBlastFamilyGetMaxActorCount(family->blast, nullptr);
    for (uint32_t actor_id = 0u; actor_id < maximum; ++actor_id) {
        if (actor_from_id(family, actor_id) != nullptr) {
            actor_ids[cursor++] = actor_id;
        }
    }
    *out_count = cursor;
    return BF_DESTRUCTION_STATUS_OK;
}

int32_t bf_destruction_family_visible_chunks(
    const BFDestructionFamily* family,
    uint32_t actor_id,
    uint32_t* chunk_indices,
    uint32_t capacity,
    uint32_t* out_count) {
    if (family == nullptr || out_count == nullptr || (capacity != 0u && chunk_indices == nullptr)) {
        return BF_DESTRUCTION_STATUS_NULL_POINTER;
    }
    NvBlastActor* actor = actor_from_id(family, actor_id);
    if (actor == nullptr) {
        return BF_DESTRUCTION_STATUS_ACTOR_NOT_FOUND;
    }
    const uint32_t count = NvBlastActorGetVisibleChunkCount(actor, nullptr);
    if (capacity < count) {
        return BF_DESTRUCTION_STATUS_CAPACITY_EXCEEDED;
    }
    *out_count = NvBlastActorGetVisibleChunkIndices(chunk_indices, capacity, actor, nullptr);
    return BF_DESTRUCTION_STATUS_OK;
}

int32_t bf_destruction_family_apply_fracture(
    BFDestructionFamily* family,
    uint32_t actor_id,
    const BFDestructionFractureData* commands,
    uint32_t command_count,
    BFDestructionFractureData* events,
    uint32_t event_capacity,
    uint32_t* out_event_count) {
    if (family == nullptr || out_event_count == nullptr ||
        (command_count != 0u && commands == nullptr) ||
        (event_capacity != 0u && events == nullptr)) {
        return BF_DESTRUCTION_STATUS_NULL_POINTER;
    }
    NvBlastActor* actor = actor_from_id(family, actor_id);
    if (actor == nullptr) {
        return BF_DESTRUCTION_STATUS_ACTOR_NOT_FOUND;
    }
    NvBlastBondFractureData* bond_commands = command_count == 0u
        ? nullptr
        : new (std::nothrow) NvBlastBondFractureData[command_count];
    NvBlastChunkFractureData* chunk_commands = command_count == 0u
        ? nullptr
        : new (std::nothrow) NvBlastChunkFractureData[command_count];
    if (command_count != 0u && (bond_commands == nullptr || chunk_commands == nullptr)) {
        delete[] bond_commands;
        delete[] chunk_commands;
        return BF_DESTRUCTION_STATUS_ALLOCATION_FAILED;
    }
    uint32_t bond_count = 0u;
    uint32_t chunk_count = 0u;
    const uint32_t graph_node_count =
        NvBlastAssetGetSupportGraph(family->asset->blast, nullptr).nodeCount;
    const uint32_t asset_chunk_count =
        NvBlastAssetGetChunkCount(family->asset->blast, nullptr);
    for (uint32_t index = 0u; index < command_count; ++index) {
        const BFDestructionFractureData& command = commands[index];
        if (!(command.health > 0.0f)) {
            delete[] bond_commands;
            delete[] chunk_commands;
            return BF_DESTRUCTION_STATUS_INVALID_ARGUMENT;
        }
        if (command.kind == BF_DESTRUCTION_FRACTURE_BOND) {
            if (command.index0 >= graph_node_count || command.index1 >= graph_node_count ||
                command.index0 == command.index1) {
                delete[] bond_commands;
                delete[] chunk_commands;
                return BF_DESTRUCTION_STATUS_INVALID_ARGUMENT;
            }
            bond_commands[bond_count++] = NvBlastBondFractureData{
                command.user_data,
                command.index0,
                command.index1,
                command.health,
            };
        } else if (command.kind == BF_DESTRUCTION_FRACTURE_CHUNK) {
            if (command.index0 >= asset_chunk_count) {
                delete[] bond_commands;
                delete[] chunk_commands;
                return BF_DESTRUCTION_STATUS_INVALID_ARGUMENT;
            }
            chunk_commands[chunk_count++] = NvBlastChunkFractureData{
                command.user_data,
                command.index0,
                command.health,
            };
        } else {
            delete[] bond_commands;
            delete[] chunk_commands;
            return BF_DESTRUCTION_STATUS_INVALID_ARGUMENT;
        }
    }
    const NvBlastFractureBuffers native_commands{
        bond_count,
        chunk_count,
        bond_commands,
        chunk_commands,
    };
    const int32_t status = apply_native_fracture(
        family,
        actor,
        native_commands,
        events,
        event_capacity,
        out_event_count);
    delete[] bond_commands;
    delete[] chunk_commands;
    return status;
}

int32_t bf_destruction_family_split_actor(
    BFDestructionFamily* family,
    uint32_t actor_id,
    uint32_t* new_actor_ids,
    uint32_t capacity,
    uint32_t* out_count) {
    if (family == nullptr || out_count == nullptr || (capacity != 0u && new_actor_ids == nullptr)) {
        return BF_DESTRUCTION_STATUS_NULL_POINTER;
    }
    NvBlastActor* actor = actor_from_id(family, actor_id);
    if (actor == nullptr) {
        return BF_DESTRUCTION_STATUS_ACTOR_NOT_FOUND;
    }
    *out_count = 0u;
    if (!NvBlastActorIsSplitRequired(actor, nullptr)) {
        return BF_DESTRUCTION_STATUS_OK;
    }
    const uint32_t family_maximum = NvBlastFamilyGetMaxActorCount(family->blast, nullptr);
    if (capacity < family_maximum) {
        return BF_DESTRUCTION_STATUS_CAPACITY_EXCEEDED;
    }
    const uint32_t maximum = NvBlastActorGetMaxActorCountForSplit(actor, nullptr);
    NvBlastActor** new_actors = maximum == 0u
        ? nullptr
        : new (std::nothrow) NvBlastActor*[maximum];
    const size_t scratch_size = NvBlastActorGetRequiredScratchForSplit(actor, nullptr);
    void* scratch = scratch_size == 0u ? nullptr : allocate_aligned(scratch_size);
    if ((maximum != 0u && new_actors == nullptr) || (scratch_size != 0u && scratch == nullptr)) {
        delete[] new_actors;
        free_aligned(scratch);
        return BF_DESTRUCTION_STATUS_ALLOCATION_FAILED;
    }
#if BLACKFLOWER_BLAST_HAS_STRESS
    if (family->stress != nullptr) {
        family->stress->notifyActorDestroyed(*actor);
    }
#endif
    NvBlastActorSplitEvent split_event{nullptr, new_actors};
    const uint32_t count = NvBlastActorSplit(
        &split_event,
        actor,
        maximum,
        scratch,
        nullptr,
        nullptr);
    free_aligned(scratch);
    for (uint32_t index = 0u; index < count; ++index) {
        new_actor_ids[index] = NvBlastActorGetIndex(new_actors[index], nullptr);
#if BLACKFLOWER_BLAST_HAS_STRESS
        if (family->stress != nullptr) {
            family->stress->notifyActorCreated(*new_actors[index]);
        }
#endif
    }
#if BLACKFLOWER_BLAST_HAS_STRESS
    if (count == 0u && family->stress != nullptr) {
        family->stress->notifyActorCreated(*actor);
    }
#endif
    delete[] new_actors;
    *out_count = count;
    return BF_DESTRUCTION_STATUS_OK;
}

int32_t bf_destruction_family_enable_stress(
    BFDestructionFamily* family,
    const BFDestructionStressSettings* settings,
    float density) {
    if (family == nullptr || settings == nullptr) {
        return BF_DESTRUCTION_STATUS_NULL_POINTER;
    }
#if BLACKFLOWER_BLAST_HAS_STRESS
    if (!(density > 0.0f) || settings->max_solver_iterations_per_frame == 0u ||
        !(settings->compression_elastic_limit >= 0.0f) ||
        !(settings->compression_fatal_limit > settings->compression_elastic_limit)) {
        return BF_DESTRUCTION_STATUS_INVALID_ARGUMENT;
    }
    if (family->stress != nullptr) {
        family->stress->release();
    }
    Nv::Blast::ExtStressSolverSettings native_settings;
    native_settings.maxSolverIterationsPerFrame = settings->max_solver_iterations_per_frame;
    native_settings.graphReductionLevel = settings->graph_reduction_level;
    native_settings.compressionElasticLimit = settings->compression_elastic_limit;
    native_settings.compressionFatalLimit = settings->compression_fatal_limit;
    native_settings.tensionElasticLimit = settings->tension_elastic_limit;
    native_settings.tensionFatalLimit = settings->tension_fatal_limit;
    native_settings.shearElasticLimit = settings->shear_elastic_limit;
    native_settings.shearFatalLimit = settings->shear_fatal_limit;
    family->stress = Nv::Blast::ExtStressSolver::create(*family->blast, native_settings);
    if (family->stress == nullptr) {
        return BF_DESTRUCTION_STATUS_STRESS_FAILED;
    }
    family->stress->setAllNodesInfoFromLL(density);
    const uint32_t maximum = NvBlastFamilyGetMaxActorCount(family->blast, nullptr);
    for (uint32_t actor_id = 0u; actor_id < maximum; ++actor_id) {
        NvBlastActor* actor = actor_from_id(family, actor_id);
        if (actor != nullptr) {
            family->stress->notifyActorCreated(*actor);
        }
    }
    return BF_DESTRUCTION_STATUS_OK;
#else
    (void)density;
    return BF_DESTRUCTION_STATUS_STRESS_UNAVAILABLE;
#endif
}

int32_t bf_destruction_family_stress_add_force(
    BFDestructionFamily* family,
    uint32_t graph_node_index,
    BFDestructionVec3 force,
    uint32_t mode) {
    if (family == nullptr) {
        return BF_DESTRUCTION_STATUS_NULL_POINTER;
    }
#if BLACKFLOWER_BLAST_HAS_STRESS
    if (family->stress == nullptr) {
        return BF_DESTRUCTION_STATUS_STRESS_FAILED;
    }
    const uint32_t graph_node_count =
        NvBlastAssetGetSupportGraph(family->asset->blast, nullptr).nodeCount;
    if (graph_node_index >= graph_node_count) {
        return BF_DESTRUCTION_STATUS_INVALID_ARGUMENT;
    }
    if (mode != BF_DESTRUCTION_FORCE && mode != BF_DESTRUCTION_ACCELERATION) {
        return BF_DESTRUCTION_STATUS_INVALID_ARGUMENT;
    }
    const Nv::Blast::ExtForceMode::Enum native_mode = mode == BF_DESTRUCTION_FORCE
        ? Nv::Blast::ExtForceMode::FORCE
        : Nv::Blast::ExtForceMode::ACCELERATION;
    family->stress->addForce(graph_node_index, NvcVec3{force.x, force.y, force.z}, native_mode);
    return BF_DESTRUCTION_STATUS_OK;
#else
    (void)graph_node_index;
    (void)force;
    (void)mode;
    return BF_DESTRUCTION_STATUS_STRESS_UNAVAILABLE;
#endif
}

int32_t bf_destruction_family_stress_update(BFDestructionFamily* family) {
    if (family == nullptr) {
        return BF_DESTRUCTION_STATUS_NULL_POINTER;
    }
#if BLACKFLOWER_BLAST_HAS_STRESS
    if (family->stress == nullptr) {
        return BF_DESTRUCTION_STATUS_STRESS_FAILED;
    }
    family->stress->update();
    return BF_DESTRUCTION_STATUS_OK;
#else
    return BF_DESTRUCTION_STATUS_STRESS_UNAVAILABLE;
#endif
}

int32_t bf_destruction_family_apply_stress(
    BFDestructionFamily* family,
    uint32_t actor_id,
    BFDestructionFractureData* events,
    uint32_t event_capacity,
    uint32_t* out_event_count) {
    if (family == nullptr || out_event_count == nullptr ||
        (event_capacity != 0u && events == nullptr)) {
        return BF_DESTRUCTION_STATUS_NULL_POINTER;
    }
#if BLACKFLOWER_BLAST_HAS_STRESS
    if (family->stress == nullptr) {
        return BF_DESTRUCTION_STATUS_STRESS_FAILED;
    }
    NvBlastActor* actor = actor_from_id(family, actor_id);
    if (actor == nullptr) {
        return BF_DESTRUCTION_STATUS_ACTOR_NOT_FOUND;
    }
    NvBlastFractureBuffers commands{};
    family->stress->generateFractureCommands(*actor, commands);
    return apply_native_fracture(
        family,
        actor,
        commands,
        events,
        event_capacity,
        out_event_count);
#else
    (void)actor_id;
    return BF_DESTRUCTION_STATUS_STRESS_UNAVAILABLE;
#endif
}

int32_t bf_destruction_family_stress_stats(
    const BFDestructionFamily* family,
    BFDestructionStressStats* out_stats) {
    if (family == nullptr || out_stats == nullptr) {
        return BF_DESTRUCTION_STATUS_NULL_POINTER;
    }
#if BLACKFLOWER_BLAST_HAS_STRESS
    if (family->stress == nullptr) {
        return BF_DESTRUCTION_STATUS_STRESS_FAILED;
    }
    *out_stats = BFDestructionStressStats{
        family->stress->getFrameCount(),
        family->stress->getBondCount(),
        family->stress->getOverstressedBondCount(),
        family->stress->getStressErrorLinear(),
        family->stress->getStressErrorAngular(),
        static_cast<uint8_t>(family->stress->converged() ? 1u : 0u),
    };
    return BF_DESTRUCTION_STATUS_OK;
#else
    return BF_DESTRUCTION_STATUS_STRESS_UNAVAILABLE;
#endif
}

} // extern "C"
