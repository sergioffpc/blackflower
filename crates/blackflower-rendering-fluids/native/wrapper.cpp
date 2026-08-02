#include "wrapper.h"

#include "NvFlowContext.h"
#include "NvFlowExt.h"

#include <cstring>
#include <new>

namespace {

struct Buffer {
    BFFlowResourceId id;
    bool alive;
    Buffer* next;
};

struct BufferTransient {
    Buffer* buffer;
    NvFlowFormat format;
    NvFlowUint structureStride;
    NvFlowUint64 id;
    BufferTransient* next;
};

struct BufferAcquire {
    BufferTransient* transient;
    BufferAcquire* next;
};

struct Texture {
    BFFlowResourceId id;
    bool alive;
    Texture* next;
};

struct TextureTransient {
    Texture* texture;
    NvFlowFormat format;
    NvFlowUint64 id;
    TextureTransient* next;
};

struct TextureAcquire {
    TextureTransient* transient;
    TextureAcquire* next;
};

struct Sampler {
    BFFlowResourceId id;
    bool alive;
    Sampler* next;
};

struct Pipeline {
    BFFlowResourceId id;
    bool alive;
    Pipeline* next;
};

} // namespace

struct BFFlowContext {
    BFFlowBackendCallbacks callbacks;
    NvFlowContextInterface backend_interface;
    NvFlowContextOptInterface* opt_interface;
    NvFlowContextOpt* opt;
    NvFlowContextInterface* flow_interface;
    NvFlowContext* flow_context;
    Buffer* buffers;
    BufferTransient* buffer_transients;
    BufferAcquire* buffer_acquires;
    Texture* textures;
    TextureTransient* texture_transients;
    TextureAcquire* texture_acquires;
    Sampler* samplers;
    Pipeline* pipelines;
    NvFlowUint64 next_transient_id;
    bool backend_failed;
};

namespace {

BFFlowContext* cast_context(NvFlowContext* context) {
    return reinterpret_cast<BFFlowContext*>(context);
}

template <typename T, typename... Args>
T* prepend(T*& head, Args... args) {
    T* value = new (std::nothrow) T{args..., head};
    if (value != nullptr) {
        head = value;
    }
    return value;
}

template <typename T>
void release_list(T*& head) {
    while (head != nullptr) {
        T* next = head->next;
        delete head;
        head = next;
    }
}

void mark_failed(BFFlowContext* context) {
    context->backend_failed = true;
}

void get_context_config(NvFlowContext*, NvFlowContextConfig* config) {
    if (config != nullptr) {
        config->api = eNvFlowContextApi_vulkan;
        config->textureBinding = eNvFlowTextureBindingType_separateSampler;
    }
}

NvFlowBool32 is_feature_supported(NvFlowContext* native, NvFlowContextFeature feature) {
    BFFlowContext* context = cast_context(native);
    if (context->callbacks.is_feature_supported == nullptr) {
        return NV_FLOW_FALSE;
    }
    return context->callbacks.is_feature_supported(
        context->callbacks.userdata,
        static_cast<uint32_t>(feature)) != 0u ? NV_FLOW_TRUE : NV_FLOW_FALSE;
}

NvFlowUint64 get_current_frame(NvFlowContext* native) {
    BFFlowContext* context = cast_context(native);
    return context->callbacks.get_current_frame(context->callbacks.userdata);
}

NvFlowUint64 get_last_completed_frame(NvFlowContext* native) {
    BFFlowContext* context = cast_context(native);
    return context->callbacks.get_last_completed_frame(context->callbacks.userdata);
}

NvFlowLogPrint_t get_log_print(NvFlowContext*) {
    return nullptr;
}

void execute_tasks(
    NvFlowContext*,
    NvFlowUint task_count,
    NvFlowUint,
    NvFlowContextThreadPoolTask_t task,
    void* userdata) {
    if (task == nullptr) {
        return;
    }
    for (NvFlowUint index = 0u; index < task_count; ++index) {
        task(index, 0u, nullptr, userdata);
    }
}

NvFlowBuffer* create_buffer(
    NvFlowContext* native,
    NvFlowMemoryType memory_type,
    const NvFlowBufferDesc* desc) {
    BFFlowContext* context = cast_context(native);
    if (desc == nullptr) {
        mark_failed(context);
        return nullptr;
    }
    const BFFlowBufferDesc bridge{
        desc->usageFlags,
        static_cast<uint32_t>(desc->format),
        desc->structureStride,
        desc->sizeInBytes,
        static_cast<uint32_t>(memory_type),
    };
    const BFFlowResourceId id = context->callbacks.create_buffer(
        context->callbacks.userdata,
        &bridge);
    if (id == 0u) {
        mark_failed(context);
        return nullptr;
    }
    Buffer* buffer = prepend(context->buffers, id, true);
    if (buffer == nullptr) {
        context->callbacks.destroy_buffer(context->callbacks.userdata, id);
        mark_failed(context);
    }
    return reinterpret_cast<NvFlowBuffer*>(buffer);
}

void destroy_buffer(NvFlowContext* native, NvFlowBuffer* opaque) {
    BFFlowContext* context = cast_context(native);
    Buffer* buffer = reinterpret_cast<Buffer*>(opaque);
    if (buffer != nullptr && buffer->alive) {
        context->callbacks.destroy_buffer(context->callbacks.userdata, buffer->id);
        buffer->alive = false;
    }
}

NvFlowBufferTransient* register_buffer_transient(
    NvFlowContext* native,
    NvFlowBuffer* opaque) {
    BFFlowContext* context = cast_context(native);
    Buffer* buffer = reinterpret_cast<Buffer*>(opaque);
    if (buffer == nullptr) {
        mark_failed(context);
        return nullptr;
    }
    BufferTransient* transient = prepend(
        context->buffer_transients,
        buffer,
        eNvFlowFormat_unknown,
        0u,
        context->next_transient_id++);
    if (transient == nullptr) {
        mark_failed(context);
    }
    return reinterpret_cast<NvFlowBufferTransient*>(transient);
}

NvFlowBufferTransient* get_buffer_transient(NvFlowContext* native, const NvFlowBufferDesc* desc) {
    NvFlowBuffer* buffer = create_buffer(native, eNvFlowMemoryType_device, desc);
    return buffer == nullptr ? nullptr : register_buffer_transient(native, buffer);
}

NvFlowBufferTransient* alias_buffer_transient(
    NvFlowContext* native,
    NvFlowBufferTransient* opaque,
    NvFlowFormat format,
    NvFlowUint structure_stride) {
    BFFlowContext* context = cast_context(native);
    BufferTransient* source = reinterpret_cast<BufferTransient*>(opaque);
    if (source == nullptr) {
        mark_failed(context);
        return nullptr;
    }
    BufferTransient* alias = prepend(
        context->buffer_transients,
        source->buffer,
        format,
        structure_stride,
        context->next_transient_id++);
    if (alias == nullptr) {
        mark_failed(context);
    }
    return reinterpret_cast<NvFlowBufferTransient*>(alias);
}

NvFlowBufferAcquire* enqueue_acquire_buffer(
    NvFlowContext* native,
    NvFlowBufferTransient* opaque) {
    BFFlowContext* context = cast_context(native);
    BufferTransient* transient = reinterpret_cast<BufferTransient*>(opaque);
    BufferAcquire* acquire = prepend(context->buffer_acquires, transient);
    if (acquire == nullptr) {
        mark_failed(context);
    }
    return reinterpret_cast<NvFlowBufferAcquire*>(acquire);
}

NvFlowBool32 get_acquired_buffer(
    NvFlowContext*,
    NvFlowBufferAcquire* opaque,
    NvFlowBuffer** out_buffer) {
    BufferAcquire* acquire = reinterpret_cast<BufferAcquire*>(opaque);
    if (acquire == nullptr || acquire->transient == nullptr || out_buffer == nullptr) {
        return NV_FLOW_FALSE;
    }
    *out_buffer = reinterpret_cast<NvFlowBuffer*>(acquire->transient->buffer);
    return NV_FLOW_TRUE;
}

void* map_buffer(NvFlowContext* native, NvFlowBuffer* opaque) {
    BFFlowContext* context = cast_context(native);
    Buffer* buffer = reinterpret_cast<Buffer*>(opaque);
    if (buffer == nullptr) {
        mark_failed(context);
        return nullptr;
    }
    void* mapped = context->callbacks.map_buffer(context->callbacks.userdata, buffer->id);
    if (mapped == nullptr) {
        mark_failed(context);
    }
    return mapped;
}

void unmap_buffer(NvFlowContext* native, NvFlowBuffer* opaque) {
    BFFlowContext* context = cast_context(native);
    Buffer* buffer = reinterpret_cast<Buffer*>(opaque);
    if (buffer != nullptr) {
        context->callbacks.unmap_buffer(context->callbacks.userdata, buffer->id);
    }
}

NvFlowBufferTransient* get_buffer_transient_by_id(NvFlowContext* native, NvFlowUint64 id) {
    BFFlowContext* context = cast_context(native);
    for (BufferTransient* value = context->buffer_transients; value != nullptr; value = value->next) {
        if (value->id == id) {
            return reinterpret_cast<NvFlowBufferTransient*>(value);
        }
    }
    return nullptr;
}

void get_buffer_external_handle(NvFlowContext*, NvFlowBuffer*, NvFlowInteropHandle* handle) {
    if (handle != nullptr) {
        *handle = NvFlowInteropHandle_default;
    }
}

void close_buffer_external_handle(NvFlowContext*, NvFlowBuffer*, const NvFlowInteropHandle*) {}

NvFlowBuffer* create_buffer_from_external_handle(
    NvFlowContext*,
    const NvFlowBufferDesc*,
    const NvFlowInteropHandle*) {
    return nullptr;
}

NvFlowTexture* create_texture(NvFlowContext* native, const NvFlowTextureDesc* desc) {
    BFFlowContext* context = cast_context(native);
    if (desc == nullptr) {
        mark_failed(context);
        return nullptr;
    }
    const BFFlowTextureDesc bridge{
        static_cast<uint32_t>(desc->textureType),
        desc->usageFlags,
        static_cast<uint32_t>(desc->format),
        desc->width,
        desc->height,
        desc->depth,
        desc->mipLevels,
        {
            desc->optimizedClearValue.x,
            desc->optimizedClearValue.y,
            desc->optimizedClearValue.z,
            desc->optimizedClearValue.w,
        },
    };
    const BFFlowResourceId id = context->callbacks.create_texture(
        context->callbacks.userdata,
        &bridge);
    if (id == 0u) {
        mark_failed(context);
        return nullptr;
    }
    Texture* texture = prepend(context->textures, id, true);
    if (texture == nullptr) {
        context->callbacks.destroy_texture(context->callbacks.userdata, id);
        mark_failed(context);
    }
    return reinterpret_cast<NvFlowTexture*>(texture);
}

void destroy_texture(NvFlowContext* native, NvFlowTexture* opaque) {
    BFFlowContext* context = cast_context(native);
    Texture* texture = reinterpret_cast<Texture*>(opaque);
    if (texture != nullptr && texture->alive) {
        context->callbacks.destroy_texture(context->callbacks.userdata, texture->id);
        texture->alive = false;
    }
}

NvFlowTextureTransient* register_texture_transient(
    NvFlowContext* native,
    NvFlowTexture* opaque) {
    BFFlowContext* context = cast_context(native);
    Texture* texture = reinterpret_cast<Texture*>(opaque);
    if (texture == nullptr) {
        mark_failed(context);
        return nullptr;
    }
    TextureTransient* transient = prepend(
        context->texture_transients,
        texture,
        eNvFlowFormat_unknown,
        context->next_transient_id++);
    if (transient == nullptr) {
        mark_failed(context);
    }
    return reinterpret_cast<NvFlowTextureTransient*>(transient);
}

NvFlowTextureTransient* get_texture_transient(
    NvFlowContext* native,
    const NvFlowTextureDesc* desc) {
    NvFlowTexture* texture = create_texture(native, desc);
    return texture == nullptr ? nullptr : register_texture_transient(native, texture);
}

NvFlowTextureTransient* alias_texture_transient(
    NvFlowContext* native,
    NvFlowTextureTransient* opaque,
    NvFlowFormat format) {
    BFFlowContext* context = cast_context(native);
    TextureTransient* source = reinterpret_cast<TextureTransient*>(opaque);
    if (source == nullptr) {
        mark_failed(context);
        return nullptr;
    }
    TextureTransient* alias = prepend(
        context->texture_transients,
        source->texture,
        format,
        context->next_transient_id++);
    if (alias == nullptr) {
        mark_failed(context);
    }
    return reinterpret_cast<NvFlowTextureTransient*>(alias);
}

NvFlowTextureAcquire* enqueue_acquire_texture(
    NvFlowContext* native,
    NvFlowTextureTransient* opaque) {
    BFFlowContext* context = cast_context(native);
    TextureTransient* transient = reinterpret_cast<TextureTransient*>(opaque);
    TextureAcquire* acquire = prepend(context->texture_acquires, transient);
    if (acquire == nullptr) {
        mark_failed(context);
    }
    return reinterpret_cast<NvFlowTextureAcquire*>(acquire);
}

NvFlowBool32 get_acquired_texture(
    NvFlowContext*,
    NvFlowTextureAcquire* opaque,
    NvFlowTexture** out_texture) {
    TextureAcquire* acquire = reinterpret_cast<TextureAcquire*>(opaque);
    if (acquire == nullptr || acquire->transient == nullptr || out_texture == nullptr) {
        return NV_FLOW_FALSE;
    }
    *out_texture = reinterpret_cast<NvFlowTexture*>(acquire->transient->texture);
    return NV_FLOW_TRUE;
}

NvFlowTextureTransient* get_texture_transient_by_id(NvFlowContext* native, NvFlowUint64 id) {
    BFFlowContext* context = cast_context(native);
    for (TextureTransient* value = context->texture_transients; value != nullptr; value = value->next) {
        if (value->id == id) {
            return reinterpret_cast<NvFlowTextureTransient*>(value);
        }
    }
    return nullptr;
}

NvFlowSampler* create_sampler(NvFlowContext* native, const NvFlowSamplerDesc* desc) {
    BFFlowContext* context = cast_context(native);
    if (desc == nullptr) {
        mark_failed(context);
        return nullptr;
    }
    const BFFlowSamplerDesc bridge{
        static_cast<uint32_t>(desc->addressModeU),
        static_cast<uint32_t>(desc->addressModeV),
        static_cast<uint32_t>(desc->addressModeW),
        static_cast<uint32_t>(desc->filterMode),
    };
    const BFFlowResourceId id = context->callbacks.create_sampler(
        context->callbacks.userdata,
        &bridge);
    if (id == 0u) {
        mark_failed(context);
        return nullptr;
    }
    Sampler* sampler = prepend(context->samplers, id, true);
    if (sampler == nullptr) {
        context->callbacks.destroy_sampler(context->callbacks.userdata, id);
        mark_failed(context);
    }
    return reinterpret_cast<NvFlowSampler*>(sampler);
}

NvFlowSampler* get_default_sampler(NvFlowContext* native) {
    const NvFlowSamplerDesc desc{
        eNvFlowSamplerAddressMode_border,
        eNvFlowSamplerAddressMode_border,
        eNvFlowSamplerAddressMode_border,
        eNvFlowSamplerFilterMode_point,
    };
    return create_sampler(native, &desc);
}

void destroy_sampler(NvFlowContext* native, NvFlowSampler* opaque) {
    BFFlowContext* context = cast_context(native);
    Sampler* sampler = reinterpret_cast<Sampler*>(opaque);
    if (sampler != nullptr && sampler->alive) {
        context->callbacks.destroy_sampler(context->callbacks.userdata, sampler->id);
        sampler->alive = false;
    }
}

NvFlowComputePipeline* create_compute_pipeline(
    NvFlowContext* native,
    const NvFlowComputePipelineDesc* desc) {
    BFFlowContext* context = cast_context(native);
    if (desc == nullptr) {
        mark_failed(context);
        return nullptr;
    }
    BFFlowBindingDesc* bindings = desc->numBindingDescs == 0u
        ? nullptr
        : new (std::nothrow) BFFlowBindingDesc[desc->numBindingDescs];
    if (desc->numBindingDescs != 0u && bindings == nullptr) {
        mark_failed(context);
        return nullptr;
    }
    for (NvFlowUint index = 0u; index < desc->numBindingDescs; ++index) {
        const NvFlowBindingDesc& source = desc->bindingDescs[index];
        bindings[index] = BFFlowBindingDesc{
            static_cast<uint32_t>(source.type),
            source.bindingDesc.vulkan.binding,
            source.bindingDesc.vulkan.descriptorCount,
            source.bindingDesc.vulkan.set,
        };
    }
    const BFFlowComputePipelineDesc bridge{
        bindings,
        desc->numBindingDescs,
        static_cast<const uint8_t*>(desc->bytecode.data),
        desc->bytecode.sizeInBytes,
    };
    const BFFlowResourceId id = context->callbacks.create_compute_pipeline(
        context->callbacks.userdata,
        &bridge);
    delete[] bindings;
    if (id == 0u) {
        mark_failed(context);
        return nullptr;
    }
    Pipeline* pipeline = prepend(context->pipelines, id, true);
    if (pipeline == nullptr) {
        context->callbacks.destroy_compute_pipeline(context->callbacks.userdata, id);
        mark_failed(context);
    }
    return reinterpret_cast<NvFlowComputePipeline*>(pipeline);
}

void destroy_compute_pipeline(NvFlowContext* native, NvFlowComputePipeline* opaque) {
    BFFlowContext* context = cast_context(native);
    Pipeline* pipeline = reinterpret_cast<Pipeline*>(opaque);
    if (pipeline != nullptr && pipeline->alive) {
        context->callbacks.destroy_compute_pipeline(context->callbacks.userdata, pipeline->id);
        pipeline->alive = false;
    }
}

BFFlowResourceId buffer_id(NvFlowBufferTransient* opaque) {
    BufferTransient* transient = reinterpret_cast<BufferTransient*>(opaque);
    return transient == nullptr || transient->buffer == nullptr ? 0u : transient->buffer->id;
}

BFFlowResourceId texture_id(NvFlowTextureTransient* opaque) {
    TextureTransient* transient = reinterpret_cast<TextureTransient*>(opaque);
    return transient == nullptr || transient->texture == nullptr ? 0u : transient->texture->id;
}

void add_pass_compute(NvFlowContext* native, const NvFlowPassComputeParams* params) {
    BFFlowContext* context = cast_context(native);
    if (params == nullptr || params->pipeline == nullptr) {
        mark_failed(context);
        return;
    }
    BFFlowResourceBinding* resources = params->numDescriptorWrites == 0u
        ? nullptr
        : new (std::nothrow) BFFlowResourceBinding[params->numDescriptorWrites];
    if (params->numDescriptorWrites != 0u && resources == nullptr) {
        mark_failed(context);
        return;
    }
    for (NvFlowUint index = 0u; index < params->numDescriptorWrites; ++index) {
        const NvFlowDescriptorWrite& write = params->descriptorWrites[index];
        const NvFlowResource& resource = params->resources[index];
        Sampler* sampler = reinterpret_cast<Sampler*>(resource.sampler);
        resources[index] = BFFlowResourceBinding{
            static_cast<uint32_t>(write.type),
            write.write.vulkan.binding,
            write.write.vulkan.arrayIndex,
            write.write.vulkan.set,
            buffer_id(resource.bufferTransient),
            texture_id(resource.textureTransient),
            sampler == nullptr ? 0u : sampler->id,
        };
    }
    Pipeline* pipeline = reinterpret_cast<Pipeline*>(params->pipeline);
    const BFFlowComputePass bridge{
        pipeline->id,
        params->gridDim.x,
        params->gridDim.y,
        params->gridDim.z,
        resources,
        params->numDescriptorWrites,
        params->debugLabel,
    };
    if (context->callbacks.add_compute_pass(context->callbacks.userdata, &bridge) == 0u) {
        mark_failed(context);
    }
    delete[] resources;
}

void add_pass_copy_buffer(NvFlowContext* native, const NvFlowPassCopyBufferParams* params) {
    BFFlowContext* context = cast_context(native);
    const BFFlowCopyBufferPass bridge{
        buffer_id(params->src),
        buffer_id(params->dst),
        params->srcOffset,
        params->dstOffset,
        params->numBytes,
        params->debugLabel,
    };
    if (context->callbacks.add_copy_buffer_pass(context->callbacks.userdata, &bridge) == 0u) {
        mark_failed(context);
    }
}

BFFlowBufferTextureCopyPass buffer_texture_copy(
    BFFlowResourceId buffer,
    BFFlowResourceId texture,
    NvFlowUint64 buffer_offset,
    NvFlowUint buffer_row_pitch,
    NvFlowUint buffer_depth_pitch,
    NvFlowUint mip_level,
    NvFlowUint3 offset,
    NvFlowUint3 extent,
    const char* label) {
    return BFFlowBufferTextureCopyPass{
        buffer,
        texture,
        buffer_offset,
        buffer_row_pitch,
        buffer_depth_pitch,
        mip_level,
        {offset.x, offset.y, offset.z},
        {extent.x, extent.y, extent.z},
        label,
    };
}

void add_pass_copy_buffer_to_texture(
    NvFlowContext* native,
    const NvFlowPassCopyBufferToTextureParams* params) {
    BFFlowContext* context = cast_context(native);
    const BFFlowBufferTextureCopyPass bridge = buffer_texture_copy(
        buffer_id(params->src),
        texture_id(params->dst),
        params->bufferOffset,
        params->bufferRowPitch,
        params->bufferDepthPitch,
        params->textureMipLevel,
        params->textureOffset,
        params->textureExtent,
        params->debugLabel);
    if (context->callbacks.add_copy_buffer_to_texture_pass(
            context->callbacks.userdata,
            &bridge) == 0u) {
        mark_failed(context);
    }
}

void add_pass_copy_texture_to_buffer(
    NvFlowContext* native,
    const NvFlowPassCopyTextureToBufferParams* params) {
    BFFlowContext* context = cast_context(native);
    const BFFlowBufferTextureCopyPass bridge = buffer_texture_copy(
        buffer_id(params->dst),
        texture_id(params->src),
        params->bufferOffset,
        params->bufferRowPitch,
        params->bufferDepthPitch,
        params->textureMipLevel,
        params->textureOffset,
        params->textureExtent,
        params->debugLabel);
    if (context->callbacks.add_copy_texture_to_buffer_pass(
            context->callbacks.userdata,
            &bridge) == 0u) {
        mark_failed(context);
    }
}

void add_pass_copy_texture(NvFlowContext* native, const NvFlowPassCopyTextureParams* params) {
    BFFlowContext* context = cast_context(native);
    const BFFlowCopyTexturePass bridge{
        texture_id(params->src),
        texture_id(params->dst),
        params->srcMipLevel,
        {params->srcOffset.x, params->srcOffset.y, params->srcOffset.z},
        params->dstMipLevel,
        {params->dstOffset.x, params->dstOffset.y, params->dstOffset.z},
        {params->extent.x, params->extent.y, params->extent.z},
        params->debugLabel,
    };
    if (context->callbacks.add_copy_texture_pass(context->callbacks.userdata, &bridge) == 0u) {
        mark_failed(context);
    }
}

bool callbacks_valid(const BFFlowBackendCallbacks& callbacks) {
    return callbacks.userdata != nullptr &&
        callbacks.get_current_frame != nullptr &&
        callbacks.get_last_completed_frame != nullptr &&
        callbacks.is_feature_supported != nullptr &&
        callbacks.create_buffer != nullptr &&
        callbacks.destroy_buffer != nullptr &&
        callbacks.map_buffer != nullptr &&
        callbacks.unmap_buffer != nullptr &&
        callbacks.create_texture != nullptr &&
        callbacks.destroy_texture != nullptr &&
        callbacks.create_sampler != nullptr &&
        callbacks.destroy_sampler != nullptr &&
        callbacks.create_compute_pipeline != nullptr &&
        callbacks.destroy_compute_pipeline != nullptr &&
        callbacks.add_compute_pass != nullptr &&
        callbacks.add_copy_buffer_pass != nullptr &&
        callbacks.add_copy_buffer_to_texture_pass != nullptr &&
        callbacks.add_copy_texture_to_buffer_pass != nullptr &&
        callbacks.add_copy_texture_pass != nullptr;
}

void destroy_remaining_resources(BFFlowContext* context) {
    for (Pipeline* value = context->pipelines; value != nullptr; value = value->next) {
        if (value->alive) {
            context->callbacks.destroy_compute_pipeline(context->callbacks.userdata, value->id);
        }
    }
    for (Sampler* value = context->samplers; value != nullptr; value = value->next) {
        if (value->alive) {
            context->callbacks.destroy_sampler(context->callbacks.userdata, value->id);
        }
    }
    for (Texture* value = context->textures; value != nullptr; value = value->next) {
        if (value->alive) {
            context->callbacks.destroy_texture(context->callbacks.userdata, value->id);
        }
    }
    for (Buffer* value = context->buffers; value != nullptr; value = value->next) {
        if (value->alive) {
            context->callbacks.destroy_buffer(context->callbacks.userdata, value->id);
        }
    }
}

} // namespace

extern "C" {

const char* bf_flow_version(void) {
    return "2.2.0";
}

int32_t bf_flow_context_create(
    const BFFlowBackendCallbacks* callbacks,
    BFFlowContext** out_context) {
    if (callbacks == nullptr || out_context == nullptr) {
        return BF_FLOW_STATUS_NULL_POINTER;
    }
    *out_context = nullptr;
    if (!callbacks_valid(*callbacks)) {
        return BF_FLOW_STATUS_INVALID_ARGUMENT;
    }
    BFFlowContext* context = new (std::nothrow) BFFlowContext{};
    if (context == nullptr) {
        return BF_FLOW_STATUS_ALLOCATION_FAILED;
    }
    context->callbacks = *callbacks;
    context->next_transient_id = 1u;
    NvFlowContextInterface initialized = {
        NV_FLOW_REFLECT_INTERFACE_INIT(NvFlowContextInterface)
    };
    context->backend_interface = initialized;
    context->backend_interface.getContextConfig = get_context_config;
    context->backend_interface.isFeatureSupported = is_feature_supported;
    context->backend_interface.getCurrentFrame = get_current_frame;
    context->backend_interface.getLastFrameCompleted = get_last_completed_frame;
    context->backend_interface.getCurrentGlobalFrame = get_current_frame;
    context->backend_interface.getLastGlobalFrameCompleted = get_last_completed_frame;
    context->backend_interface.getLogPrint = get_log_print;
    context->backend_interface.executeTasks = execute_tasks;
    context->backend_interface.createBuffer = create_buffer;
    context->backend_interface.destroyBuffer = destroy_buffer;
    context->backend_interface.getBufferTransient = get_buffer_transient;
    context->backend_interface.registerBufferAsTransient = register_buffer_transient;
    context->backend_interface.aliasBufferTransient = alias_buffer_transient;
    context->backend_interface.enqueueAcquireBuffer = enqueue_acquire_buffer;
    context->backend_interface.getAcquiredBuffer = get_acquired_buffer;
    context->backend_interface.mapBuffer = map_buffer;
    context->backend_interface.unmapBuffer = unmap_buffer;
    context->backend_interface.getBufferTransientById = get_buffer_transient_by_id;
    context->backend_interface.getBufferExternalHandle = get_buffer_external_handle;
    context->backend_interface.closeBufferExternalHandle = close_buffer_external_handle;
    context->backend_interface.createBufferFromExternalHandle = create_buffer_from_external_handle;
    context->backend_interface.createTexture = create_texture;
    context->backend_interface.destroyTexture = destroy_texture;
    context->backend_interface.getTextureTransient = get_texture_transient;
    context->backend_interface.registerTextureAsTransient = register_texture_transient;
    context->backend_interface.aliasTextureTransient = alias_texture_transient;
    context->backend_interface.enqueueAcquireTexture = enqueue_acquire_texture;
    context->backend_interface.getAcquiredTexture = get_acquired_texture;
    context->backend_interface.getTextureTransientById = get_texture_transient_by_id;
    context->backend_interface.createSampler = create_sampler;
    context->backend_interface.getDefaultSampler = get_default_sampler;
    context->backend_interface.destroySampler = destroy_sampler;
    context->backend_interface.createComputePipeline = create_compute_pipeline;
    context->backend_interface.destroyComputePipeline = destroy_compute_pipeline;
    context->backend_interface.addPassCompute = add_pass_compute;
    context->backend_interface.addPassCopyBuffer = add_pass_copy_buffer;
    context->backend_interface.addPassCopyBufferToTexture = add_pass_copy_buffer_to_texture;
    context->backend_interface.addPassCopyTextureToBuffer = add_pass_copy_texture_to_buffer;
    context->backend_interface.addPassCopyTexture = add_pass_copy_texture;

    context->opt_interface = NvFlowGetContextOptInterface();
    if (context->opt_interface == nullptr) {
        delete context;
        return BF_FLOW_STATUS_ALLOCATION_FAILED;
    }
    context->opt = context->opt_interface->create(
        &context->backend_interface,
        reinterpret_cast<NvFlowContext*>(context));
    if (context->opt == nullptr) {
        delete context;
        return BF_FLOW_STATUS_ALLOCATION_FAILED;
    }
    context->opt_interface->getContext(
        context->opt,
        &context->flow_interface,
        &context->flow_context);
    if (context->flow_interface == nullptr || context->flow_context == nullptr) {
        context->opt_interface->destroy(context->opt);
        delete context;
        return BF_FLOW_STATUS_ALLOCATION_FAILED;
    }
    *out_context = context;
    return BF_FLOW_STATUS_OK;
}

void bf_flow_context_destroy(BFFlowContext* context) {
    if (context == nullptr) {
        return;
    }
    if (context->opt != nullptr) {
        context->opt_interface->destroy(context->opt);
    }
    destroy_remaining_resources(context);
    release_list(context->pipelines);
    release_list(context->samplers);
    release_list(context->texture_acquires);
    release_list(context->texture_transients);
    release_list(context->textures);
    release_list(context->buffer_acquires);
    release_list(context->buffer_transients);
    release_list(context->buffers);
    delete context;
}

int32_t bf_flow_context_flush(BFFlowContext* context) {
    if (context == nullptr) {
        return BF_FLOW_STATUS_NULL_POINTER;
    }
    context->backend_failed = false;
    context->opt_interface->flush(context->opt);
    return context->backend_failed ? BF_FLOW_STATUS_BACKEND_FAILED : BF_FLOW_STATUS_OK;
}

int32_t bf_flow_context_set_min_resource_lifetime(BFFlowContext* context, uint64_t frames) {
    if (context == nullptr) {
        return BF_FLOW_STATUS_NULL_POINTER;
    }
    context->opt_interface->setResourceMinLifetime(context->opt, frames);
    return BF_FLOW_STATUS_OK;
}

int32_t bf_flow_context_validate_upload(BFFlowContext* context, uint64_t size_in_bytes) {
    if (context == nullptr) {
        return BF_FLOW_STATUS_NULL_POINTER;
    }
    if (size_in_bytes == 0u) {
        return BF_FLOW_STATUS_INVALID_ARGUMENT;
    }
    context->backend_failed = false;
    const NvFlowBufferDesc desc{
        eNvFlowBufferUsage_constantBuffer | eNvFlowBufferUsage_bufferCopySrc,
        eNvFlowFormat_unknown,
        0u,
        size_in_bytes,
    };
    NvFlowBuffer* buffer = context->flow_interface->createBuffer(
        context->flow_context,
        eNvFlowMemoryType_upload,
        &desc);
    if (buffer == nullptr) {
        return BF_FLOW_STATUS_BACKEND_FAILED;
    }
    void* mapped = context->flow_interface->mapBuffer(context->flow_context, buffer);
    if (mapped == nullptr) {
        context->flow_interface->destroyBuffer(context->flow_context, buffer);
        context->opt_interface->flush(context->opt);
        return BF_FLOW_STATUS_BACKEND_FAILED;
    }
    std::memset(mapped, 0, static_cast<size_t>(size_in_bytes));
    context->flow_interface->unmapBuffer(context->flow_context, buffer);
    context->flow_interface->destroyBuffer(context->flow_context, buffer);
    context->opt_interface->flush(context->opt);
    return context->backend_failed ? BF_FLOW_STATUS_BACKEND_FAILED : BF_FLOW_STATUS_OK;
}

} // extern "C"
