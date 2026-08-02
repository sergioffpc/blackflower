#ifndef BLACKFLOWER_FLOW_WRAPPER_H
#define BLACKFLOWER_FLOW_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BF_FLOW_STATUS_OK 0
#define BF_FLOW_STATUS_NULL_POINTER 1
#define BF_FLOW_STATUS_INVALID_ARGUMENT 2
#define BF_FLOW_STATUS_ALLOCATION_FAILED 3
#define BF_FLOW_STATUS_BACKEND_FAILED 4

#define BF_FLOW_FEATURE_ALIAS_RESOURCE_FORMATS 1
#define BF_FLOW_FEATURE_BUFFER_EXTERNAL_HANDLE 2

typedef struct BFFlowContext BFFlowContext;
typedef uint64_t BFFlowResourceId;

typedef struct BFFlowBufferDesc {
    uint32_t usage_flags;
    uint32_t format;
    uint32_t structure_stride;
    uint64_t size_in_bytes;
    uint32_t memory_type;
} BFFlowBufferDesc;

typedef struct BFFlowTextureDesc {
    uint32_t texture_type;
    uint32_t usage_flags;
    uint32_t format;
    uint32_t width;
    uint32_t height;
    uint32_t depth;
    uint32_t mip_levels;
    float optimized_clear_value[4];
} BFFlowTextureDesc;

typedef struct BFFlowSamplerDesc {
    uint32_t address_mode_u;
    uint32_t address_mode_v;
    uint32_t address_mode_w;
    uint32_t filter_mode;
} BFFlowSamplerDesc;

typedef struct BFFlowBindingDesc {
    uint32_t descriptor_type;
    uint32_t binding;
    uint32_t descriptor_count;
    uint32_t set;
} BFFlowBindingDesc;

typedef struct BFFlowComputePipelineDesc {
    const BFFlowBindingDesc* bindings;
    uint32_t binding_count;
    const uint8_t* bytecode;
    uint64_t bytecode_size;
} BFFlowComputePipelineDesc;

typedef struct BFFlowResourceBinding {
    uint32_t descriptor_type;
    uint32_t binding;
    uint32_t array_index;
    uint32_t set;
    BFFlowResourceId buffer;
    BFFlowResourceId texture;
    BFFlowResourceId sampler;
} BFFlowResourceBinding;

typedef struct BFFlowComputePass {
    BFFlowResourceId pipeline;
    uint32_t grid_x;
    uint32_t grid_y;
    uint32_t grid_z;
    const BFFlowResourceBinding* resources;
    uint32_t resource_count;
    const char* debug_label;
} BFFlowComputePass;

typedef struct BFFlowCopyBufferPass {
    BFFlowResourceId source;
    BFFlowResourceId destination;
    uint64_t source_offset;
    uint64_t destination_offset;
    uint64_t size;
    const char* debug_label;
} BFFlowCopyBufferPass;

typedef struct BFFlowBufferTextureCopyPass {
    BFFlowResourceId buffer;
    BFFlowResourceId texture;
    uint64_t buffer_offset;
    uint32_t buffer_row_pitch;
    uint32_t buffer_depth_pitch;
    uint32_t mip_level;
    uint32_t offset[3];
    uint32_t extent[3];
    const char* debug_label;
} BFFlowBufferTextureCopyPass;

typedef struct BFFlowCopyTexturePass {
    BFFlowResourceId source;
    BFFlowResourceId destination;
    uint32_t source_mip_level;
    uint32_t source_offset[3];
    uint32_t destination_mip_level;
    uint32_t destination_offset[3];
    uint32_t extent[3];
    const char* debug_label;
} BFFlowCopyTexturePass;

typedef struct BFFlowBackendCallbacks {
    void* userdata;
    uint64_t (*get_current_frame)(void* userdata);
    uint64_t (*get_last_completed_frame)(void* userdata);
    uint8_t (*is_feature_supported)(void* userdata, uint32_t feature);
    BFFlowResourceId (*create_buffer)(void* userdata, const BFFlowBufferDesc* desc);
    void (*destroy_buffer)(void* userdata, BFFlowResourceId buffer);
    void* (*map_buffer)(void* userdata, BFFlowResourceId buffer);
    void (*unmap_buffer)(void* userdata, BFFlowResourceId buffer);
    BFFlowResourceId (*create_texture)(void* userdata, const BFFlowTextureDesc* desc);
    void (*destroy_texture)(void* userdata, BFFlowResourceId texture);
    BFFlowResourceId (*create_sampler)(void* userdata, const BFFlowSamplerDesc* desc);
    void (*destroy_sampler)(void* userdata, BFFlowResourceId sampler);
    BFFlowResourceId (*create_compute_pipeline)(
        void* userdata,
        const BFFlowComputePipelineDesc* desc);
    void (*destroy_compute_pipeline)(void* userdata, BFFlowResourceId pipeline);
    uint8_t (*add_compute_pass)(void* userdata, const BFFlowComputePass* pass);
    uint8_t (*add_copy_buffer_pass)(void* userdata, const BFFlowCopyBufferPass* pass);
    uint8_t (*add_copy_buffer_to_texture_pass)(
        void* userdata,
        const BFFlowBufferTextureCopyPass* pass);
    uint8_t (*add_copy_texture_to_buffer_pass)(
        void* userdata,
        const BFFlowBufferTextureCopyPass* pass);
    uint8_t (*add_copy_texture_pass)(void* userdata, const BFFlowCopyTexturePass* pass);
} BFFlowBackendCallbacks;

const char* bf_flow_version(void);
int32_t bf_flow_context_create(
    const BFFlowBackendCallbacks* callbacks,
    BFFlowContext** out_context);
void bf_flow_context_destroy(BFFlowContext* context);
int32_t bf_flow_context_flush(BFFlowContext* context);
int32_t bf_flow_context_set_min_resource_lifetime(BFFlowContext* context, uint64_t frames);
int32_t bf_flow_context_validate_upload(BFFlowContext* context, uint64_t size_in_bytes);

#ifdef __cplusplus
}
#endif

#endif
