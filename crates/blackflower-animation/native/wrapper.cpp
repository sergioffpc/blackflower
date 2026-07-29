#include "wrapper.h"

#include <ozz/animation/runtime/animation.h>
#include <ozz/animation/runtime/blending_job.h>
#include <ozz/animation/runtime/ik_aim_job.h>
#include <ozz/animation/runtime/ik_two_bone_job.h>
#include <ozz/animation/runtime/local_to_model_job.h>
#include <ozz/animation/runtime/sampling_job.h>
#include <ozz/animation/runtime/skeleton.h>
#include <ozz/base/containers/vector.h>
#include <ozz/base/io/archive.h>
#include <ozz/base/io/stream.h>
#include <ozz/base/maths/simd_math.h>
#include <ozz/base/maths/simd_quaternion.h>
#include <ozz/base/maths/soa_transform.h>
#include <ozz/base/span.h>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <limits>
#include <memory>
#include <new>
#include <utility>

struct BFAnimationSkeleton {
    ozz::animation::Skeleton value;
};

struct BFAnimationClip {
    ozz::animation::Animation value;
};

struct BFAnimationSamplingContext {
    explicit BFAnimationSamplingContext(int max_tracks)
        : value(max_tracks) {}

    ozz::animation::SamplingJob::Context value;
};

struct BFAnimationPose {
    ozz::vector<ozz::math::SoaTransform> locals;
    ozz::vector<ozz::math::Float4x4> models;
};

namespace {

template <typename Function>
int32_t guarded(Function &&function) noexcept {
    try {
        return std::forward<Function>(function)();
    } catch (const std::bad_alloc &) {
        return BF_ANIMATION_STATUS_OUT_OF_MEMORY;
    } catch (...) {
        return BF_ANIMATION_STATUS_NATIVE_FAILURE;
    }
}

template <typename Native, typename Wrapper>
int32_t load_archive(
    const uint8_t *data,
    size_t size,
    Wrapper **out_wrapper) {
    if (data == nullptr || out_wrapper == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    *out_wrapper = nullptr;
    if (size == 0 || size > static_cast<size_t>(std::numeric_limits<int>::max())) {
        return BF_ANIMATION_STATUS_INVALID_ARGUMENT;
    }

    ozz::io::MemoryStream stream;
    if (stream.Write(data, size) != size || stream.Seek(0, ozz::io::Stream::kSet) != 0) {
        return BF_ANIMATION_STATUS_OUT_OF_MEMORY;
    }

    ozz::io::IArchive archive(&stream);
    if (!archive.TestTag<Native>()) {
        return BF_ANIMATION_STATUS_INVALID_ARCHIVE;
    }

    std::unique_ptr<Wrapper> wrapper(new Wrapper());
    archive >> wrapper->value;
    *out_wrapper = wrapper.release();
    return BF_ANIMATION_STATUS_OK;
}

int32_t update_models(
    const BFAnimationSkeleton *skeleton,
    BFAnimationPose *pose) {
    ozz::animation::LocalToModelJob job;
    job.skeleton = &skeleton->value;
    job.input = ozz::make_span(pose->locals);
    job.output = ozz::make_span(pose->models);
    return job.Run() ? BF_ANIMATION_STATUS_OK : BF_ANIMATION_STATUS_JOB_FAILED;
}

bool pose_matches(
    const BFAnimationSkeleton *skeleton,
    const BFAnimationPose *pose) {
    return pose->locals.size()
            == static_cast<size_t>(skeleton->value.num_soa_joints())
        && pose->models.size()
            == static_cast<size_t>(skeleton->value.num_joints());
}

bool finite3(const float value[3]) {
    return std::isfinite(value[0])
        && std::isfinite(value[1])
        && std::isfinite(value[2]);
}

bool finite4(const float value[4]) {
    return finite3(value) && std::isfinite(value[3]);
}

void store_lanes(ozz::math::SimdFloat4 value, float lanes[4]) {
    ozz::math::StorePtrU(value, lanes);
}

ozz::math::SimdFloat4 load_lanes(const float lanes[4]) {
    return ozz::math::simd_float4::Load(
        lanes[0], lanes[1], lanes[2], lanes[3]);
}

void multiply_joint_rotation(
    uint32_t joint,
    const ozz::math::SimdQuaternion &correction,
    ozz::span<ozz::math::SoaTransform> transforms) {
    ozz::math::SoaTransform &transform = transforms[joint / 4U];
    ozz::math::SimdQuaternion rotations[4];
    ozz::math::Transpose4x4(&transform.rotation.x, &rotations->xyzw);
    rotations[joint & 3U] = rotations[joint & 3U] * correction;
    ozz::math::Transpose4x4(&rotations->xyzw, &transform.rotation.x);
}

void copy_local_group(
    const ozz::math::SoaTransform &source,
    size_t first_joint,
    size_t joint_count,
    BFAnimationTransform *out_transforms) {
    float tx[4], ty[4], tz[4];
    float rx[4], ry[4], rz[4], rw[4];
    float sx[4], sy[4], sz[4];
    store_lanes(source.translation.x, tx);
    store_lanes(source.translation.y, ty);
    store_lanes(source.translation.z, tz);
    store_lanes(source.rotation.x, rx);
    store_lanes(source.rotation.y, ry);
    store_lanes(source.rotation.z, rz);
    store_lanes(source.rotation.w, rw);
    store_lanes(source.scale.x, sx);
    store_lanes(source.scale.y, sy);
    store_lanes(source.scale.z, sz);
    for (size_t lane = 0; lane < 4 && first_joint + lane < joint_count; ++lane) {
        BFAnimationTransform &target = out_transforms[first_joint + lane];
        target.translation[0] = tx[lane];
        target.translation[1] = ty[lane];
        target.translation[2] = tz[lane];
        target.rotation[0] = rx[lane];
        target.rotation[1] = ry[lane];
        target.rotation[2] = rz[lane];
        target.rotation[3] = rw[lane];
        target.scale[0] = sx[lane];
        target.scale[1] = sy[lane];
        target.scale[2] = sz[lane];
    }
}

void set_local_group(
    const BFAnimationTransform *transforms,
    size_t first_joint,
    size_t joint_count,
    ozz::math::SoaTransform *target) {
    float tx[4], ty[4], tz[4];
    float rx[4], ry[4], rz[4], rw[4];
    float sx[4], sy[4], sz[4];
    store_lanes(target->translation.x, tx);
    store_lanes(target->translation.y, ty);
    store_lanes(target->translation.z, tz);
    store_lanes(target->rotation.x, rx);
    store_lanes(target->rotation.y, ry);
    store_lanes(target->rotation.z, rz);
    store_lanes(target->rotation.w, rw);
    store_lanes(target->scale.x, sx);
    store_lanes(target->scale.y, sy);
    store_lanes(target->scale.z, sz);
    for (size_t lane = 0; lane < 4 && first_joint + lane < joint_count; ++lane) {
        const BFAnimationTransform &source = transforms[first_joint + lane];
        tx[lane] = source.translation[0];
        ty[lane] = source.translation[1];
        tz[lane] = source.translation[2];
        rx[lane] = source.rotation[0];
        ry[lane] = source.rotation[1];
        rz[lane] = source.rotation[2];
        rw[lane] = source.rotation[3];
        sx[lane] = source.scale[0];
        sy[lane] = source.scale[1];
        sz[lane] = source.scale[2];
    }
    target->translation.x = load_lanes(tx);
    target->translation.y = load_lanes(ty);
    target->translation.z = load_lanes(tz);
    target->rotation.x = load_lanes(rx);
    target->rotation.y = load_lanes(ry);
    target->rotation.z = load_lanes(rz);
    target->rotation.w = load_lanes(rw);
    target->scale.x = load_lanes(sx);
    target->scale.y = load_lanes(sy);
    target->scale.z = load_lanes(sz);
}

} // namespace

extern "C" BFAnimationVersion bf_animation_ozz_version() {
    return BFAnimationVersion {
        BF_OZZ_VERSION_MAJOR,
        BF_OZZ_VERSION_MINOR,
        BF_OZZ_VERSION_PATCH,
    };
}

extern "C" const char *bf_animation_simd_implementation() {
    return ozz::math::SimdImplementationName();
}

extern "C" int32_t bf_animation_skeleton_load(
    const uint8_t *data,
    size_t size,
    BFAnimationSkeleton **out_skeleton) {
    return guarded([&] {
        const int32_t status =
            load_archive<ozz::animation::Skeleton>(data, size, out_skeleton);
        if (status != BF_ANIMATION_STATUS_OK) {
            return status;
        }
        if ((*out_skeleton)->value.num_joints() <= 0) {
            delete *out_skeleton;
            *out_skeleton = nullptr;
            return BF_ANIMATION_STATUS_INVALID_ARCHIVE;
        }
        return BF_ANIMATION_STATUS_OK;
    });
}

extern "C" void bf_animation_skeleton_destroy(
    BFAnimationSkeleton *skeleton) {
    delete skeleton;
}

extern "C" uint32_t bf_animation_skeleton_joint_count(
    const BFAnimationSkeleton *skeleton) {
    if (skeleton == nullptr) {
        return 0;
    }
    return static_cast<uint32_t>(skeleton->value.num_joints());
}

extern "C" int32_t bf_animation_skeleton_joint_parent(
    const BFAnimationSkeleton *skeleton,
    uint32_t joint,
    int32_t *out_parent) {
    if (skeleton == nullptr || out_parent == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    const auto parents = skeleton->value.joint_parents();
    if (joint >= parents.size()) {
        return BF_ANIMATION_STATUS_INDEX_OUT_OF_RANGE;
    }
    *out_parent = parents[joint];
    return BF_ANIMATION_STATUS_OK;
}

extern "C" int32_t bf_animation_skeleton_joint_name(
    const BFAnimationSkeleton *skeleton,
    uint32_t joint,
    const char **out_name,
    size_t *out_length) {
    if (skeleton == nullptr || out_name == nullptr || out_length == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    const auto names = skeleton->value.joint_names();
    if (joint >= names.size()) {
        return BF_ANIMATION_STATUS_INDEX_OUT_OF_RANGE;
    }
    *out_name = names[joint];
    *out_length = std::strlen(names[joint]);
    return BF_ANIMATION_STATUS_OK;
}

extern "C" int32_t bf_animation_clip_load(
    const uint8_t *data,
    size_t size,
    BFAnimationClip **out_clip) {
    return guarded([&] {
        const int32_t status =
            load_archive<ozz::animation::Animation>(data, size, out_clip);
        if (status != BF_ANIMATION_STATUS_OK) {
            return status;
        }
        const auto &animation = (*out_clip)->value;
        if (animation.num_tracks() <= 0 || !std::isfinite(animation.duration())
            || animation.duration() <= 0.0F) {
            delete *out_clip;
            *out_clip = nullptr;
            return BF_ANIMATION_STATUS_INVALID_ARCHIVE;
        }
        return BF_ANIMATION_STATUS_OK;
    });
}

extern "C" void bf_animation_clip_destroy(BFAnimationClip *clip) {
    delete clip;
}

extern "C" float bf_animation_clip_duration(
    const BFAnimationClip *clip) {
    return clip == nullptr ? 0.0F : clip->value.duration();
}

extern "C" uint32_t bf_animation_clip_track_count(
    const BFAnimationClip *clip) {
    if (clip == nullptr) {
        return 0;
    }
    return static_cast<uint32_t>(clip->value.num_tracks());
}

extern "C" int32_t bf_animation_clip_name(
    const BFAnimationClip *clip,
    const char **out_name,
    size_t *out_length) {
    if (clip == nullptr || out_name == nullptr || out_length == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    const char *name = clip->value.name();
    *out_name = name;
    *out_length = std::strlen(name);
    return BF_ANIMATION_STATUS_OK;
}

extern "C" int32_t bf_animation_sampling_context_create(
    uint32_t max_tracks,
    BFAnimationSamplingContext **out_context) {
    if (out_context == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    *out_context = nullptr;
    if (max_tracks == 0
        || max_tracks > static_cast<uint32_t>(std::numeric_limits<int>::max())) {
        return BF_ANIMATION_STATUS_INVALID_ARGUMENT;
    }
    return guarded([&] {
        std::unique_ptr<BFAnimationSamplingContext> context(
            new BFAnimationSamplingContext(static_cast<int>(max_tracks)));
        *out_context = context.release();
        return BF_ANIMATION_STATUS_OK;
    });
}

extern "C" void bf_animation_sampling_context_destroy(
    BFAnimationSamplingContext *context) {
    delete context;
}

extern "C" uint32_t bf_animation_sampling_context_max_tracks(
    const BFAnimationSamplingContext *context) {
    if (context == nullptr) {
        return 0;
    }
    return static_cast<uint32_t>(context->value.max_tracks());
}

extern "C" void bf_animation_sampling_context_invalidate(
    BFAnimationSamplingContext *context) {
    if (context != nullptr) {
        context->value.Invalidate();
    }
}

extern "C" int32_t bf_animation_pose_create(
    const BFAnimationSkeleton *skeleton,
    BFAnimationPose **out_pose) {
    if (skeleton == nullptr || out_pose == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    *out_pose = nullptr;
    return guarded([&] {
        std::unique_ptr<BFAnimationPose> pose(new BFAnimationPose());
        const auto rest = skeleton->value.joint_rest_poses();
        pose->locals.assign(rest.begin(), rest.end());
        pose->models.resize(
            static_cast<size_t>(skeleton->value.num_joints()));
        const int32_t status = update_models(skeleton, pose.get());
        if (status != BF_ANIMATION_STATUS_OK) {
            return status;
        }
        *out_pose = pose.release();
        return BF_ANIMATION_STATUS_OK;
    });
}

extern "C" void bf_animation_pose_destroy(BFAnimationPose *pose) {
    delete pose;
}

extern "C" uint32_t bf_animation_pose_joint_count(
    const BFAnimationPose *pose) {
    if (pose == nullptr) {
        return 0;
    }
    return static_cast<uint32_t>(pose->models.size());
}

extern "C" int32_t bf_animation_pose_set_rest(
    const BFAnimationSkeleton *skeleton,
    BFAnimationPose *pose) {
    if (skeleton == nullptr || pose == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    if (!pose_matches(skeleton, pose)) {
        return BF_ANIMATION_STATUS_INCOMPATIBLE;
    }
    const auto rest = skeleton->value.joint_rest_poses();
    std::copy(rest.begin(), rest.end(), pose->locals.begin());
    return update_models(skeleton, pose);
}

extern "C" int32_t bf_animation_pose_sample(
    const BFAnimationSkeleton *skeleton,
    const BFAnimationClip *clip,
    BFAnimationSamplingContext *context,
    float ratio,
    BFAnimationPose *pose) {
    if (skeleton == nullptr || clip == nullptr || context == nullptr
        || pose == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    if (!std::isfinite(ratio) || ratio < 0.0F || ratio > 1.0F) {
        return BF_ANIMATION_STATUS_INVALID_ARGUMENT;
    }
    if (!pose_matches(skeleton, pose)
        || skeleton->value.num_joints() != clip->value.num_tracks()
        || context->value.max_tracks() < clip->value.num_tracks()) {
        return BF_ANIMATION_STATUS_INCOMPATIBLE;
    }

    ozz::animation::SamplingJob sampling;
    sampling.animation = &clip->value;
    sampling.context = &context->value;
    sampling.ratio = ratio;
    sampling.output = ozz::make_span(pose->locals);
    if (!sampling.Run()) {
        return BF_ANIMATION_STATUS_JOB_FAILED;
    }
    return update_models(skeleton, pose);
}

extern "C" int32_t bf_animation_pose_blend(
    const BFAnimationSkeleton *skeleton,
    const BFAnimationBlendLayer *layers,
    size_t layer_count,
    float threshold,
    BFAnimationPose *pose) {
    if (skeleton == nullptr || pose == nullptr
        || (layer_count > 0 && layers == nullptr)) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    if (!pose_matches(skeleton, pose)
        || !std::isfinite(threshold) || threshold <= 0.0F) {
        return BF_ANIMATION_STATUS_INVALID_ARGUMENT;
    }
    return guarded([&] {
        using Layer = ozz::animation::BlendingJob::Layer;
        ozz::vector<Layer> normal_layers;
        ozz::vector<Layer> additive_layers;
        ozz::vector<ozz::vector<ozz::math::SimdFloat4>> weight_buffers(
            layer_count);
        normal_layers.reserve(layer_count);
        additive_layers.reserve(layer_count);
        const size_t joint_count =
            static_cast<size_t>(skeleton->value.num_joints());
        const size_t soa_count =
            static_cast<size_t>(skeleton->value.num_soa_joints());
        for (size_t index = 0; index < layer_count; ++index) {
            const BFAnimationBlendLayer &source = layers[index];
            if (source.pose == nullptr || source.pose == pose
                || !pose_matches(skeleton, source.pose)
                || !std::isfinite(source.weight) || source.weight < 0.0F) {
                return BF_ANIMATION_STATUS_INCOMPATIBLE;
            }
            Layer layer;
            layer.weight = source.weight;
            layer.transform = ozz::make_span(source.pose->locals);
            if (source.joint_weight_count != 0) {
                if (source.joint_weights == nullptr
                    || source.joint_weight_count != joint_count) {
                    return BF_ANIMATION_STATUS_INCOMPATIBLE;
                }
                auto &buffer = weight_buffers[index];
                buffer.resize(soa_count);
                for (size_t group = 0; group < soa_count; ++group) {
                    float values[4] = {1.0F, 1.0F, 1.0F, 1.0F};
                    for (size_t lane = 0; lane < 4; ++lane) {
                        const size_t joint = group * 4 + lane;
                        if (joint < joint_count) {
                            const float weight = source.joint_weights[joint];
                            if (!std::isfinite(weight)
                                || weight < 0.0F || weight > 1.0F) {
                                return BF_ANIMATION_STATUS_INVALID_ARGUMENT;
                            }
                            values[lane] = weight;
                        }
                    }
                    buffer[group] = load_lanes(values);
                }
                layer.joint_weights = ozz::make_span(buffer);
            }
            if (source.additive == 0) {
                normal_layers.push_back(layer);
            } else {
                additive_layers.push_back(layer);
            }
        }

        ozz::animation::BlendingJob job;
        job.threshold = threshold;
        job.layers = ozz::make_span(normal_layers);
        job.additive_layers = ozz::make_span(additive_layers);
        job.rest_pose = skeleton->value.joint_rest_poses();
        job.output = ozz::make_span(pose->locals);
        if (!job.Run()) {
            return BF_ANIMATION_STATUS_JOB_FAILED;
        }
        return update_models(skeleton, pose);
    });
}

extern "C" int32_t bf_animation_pose_copy_local_transforms(
    const BFAnimationPose *pose,
    BFAnimationTransform *out_transforms,
    size_t transform_count) {
    if (pose == nullptr || out_transforms == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    if (transform_count != pose->models.size()) {
        return BF_ANIMATION_STATUS_INCOMPATIBLE;
    }
    for (size_t group = 0; group < pose->locals.size(); ++group) {
        copy_local_group(
            pose->locals[group], group * 4, transform_count, out_transforms);
    }
    return BF_ANIMATION_STATUS_OK;
}

extern "C" int32_t bf_animation_pose_set_local_transforms(
    const BFAnimationSkeleton *skeleton,
    const BFAnimationTransform *transforms,
    size_t transform_count,
    BFAnimationPose *pose) {
    if (skeleton == nullptr || transforms == nullptr || pose == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    if (!pose_matches(skeleton, pose) || transform_count != pose->models.size()) {
        return BF_ANIMATION_STATUS_INCOMPATIBLE;
    }
    for (size_t joint = 0; joint < transform_count; ++joint) {
        const BFAnimationTransform &transform = transforms[joint];
        if (!finite3(transform.translation)
            || !finite4(transform.rotation)
            || !finite3(transform.scale)) {
            return BF_ANIMATION_STATUS_INVALID_ARGUMENT;
        }
    }
    for (size_t group = 0; group < pose->locals.size(); ++group) {
        set_local_group(
            transforms, group * 4, transform_count, &pose->locals[group]);
    }
    return update_models(skeleton, pose);
}

extern "C" int32_t bf_animation_pose_apply_aim_ik(
    const BFAnimationSkeleton *skeleton,
    const BFAnimationAimIk *configuration,
    BFAnimationPose *pose,
    uint8_t *out_reached) {
    if (skeleton == nullptr || configuration == nullptr
        || pose == nullptr || out_reached == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    if (!pose_matches(skeleton, pose)
        || configuration->joint >= pose->models.size()) {
        return BF_ANIMATION_STATUS_INDEX_OUT_OF_RANGE;
    }
    ozz::animation::IKAimJob job;
    job.target = ozz::math::simd_float4::Load3PtrU(configuration->target);
    job.forward = ozz::math::simd_float4::Load3PtrU(configuration->forward);
    job.offset = ozz::math::simd_float4::Load3PtrU(configuration->offset);
    job.up = ozz::math::simd_float4::Load3PtrU(configuration->up);
    job.pole_vector =
        ozz::math::simd_float4::Load3PtrU(configuration->pole_vector);
    job.twist_angle = configuration->twist_angle;
    job.weight = configuration->weight;
    job.joint = &pose->models[configuration->joint];
    ozz::math::SimdQuaternion correction;
    job.joint_correction = &correction;
    bool reached = false;
    job.reached = &reached;
    if (!job.Run()) {
        return BF_ANIMATION_STATUS_JOB_FAILED;
    }
    multiply_joint_rotation(
        configuration->joint, correction, ozz::make_span(pose->locals));
    const int32_t status = update_models(skeleton, pose);
    *out_reached = reached ? 1U : 0U;
    return status;
}

extern "C" int32_t bf_animation_pose_apply_two_bone_ik(
    const BFAnimationSkeleton *skeleton,
    const BFAnimationTwoBoneIk *configuration,
    BFAnimationPose *pose,
    uint8_t *out_reached) {
    if (skeleton == nullptr || configuration == nullptr
        || pose == nullptr || out_reached == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    if (!pose_matches(skeleton, pose)
        || configuration->start_joint >= pose->models.size()
        || configuration->middle_joint >= pose->models.size()
        || configuration->end_joint >= pose->models.size()) {
        return BF_ANIMATION_STATUS_INDEX_OUT_OF_RANGE;
    }
    ozz::animation::IKTwoBoneJob job;
    job.target = ozz::math::simd_float4::Load3PtrU(configuration->target);
    job.mid_axis =
        ozz::math::simd_float4::Load3PtrU(configuration->middle_axis);
    job.pole_vector =
        ozz::math::simd_float4::Load3PtrU(configuration->pole_vector);
    job.twist_angle = configuration->twist_angle;
    job.soften = configuration->soften;
    job.weight = configuration->weight;
    job.start_joint = &pose->models[configuration->start_joint];
    job.mid_joint = &pose->models[configuration->middle_joint];
    job.end_joint = &pose->models[configuration->end_joint];
    ozz::math::SimdQuaternion start_correction;
    ozz::math::SimdQuaternion middle_correction;
    job.start_joint_correction = &start_correction;
    job.mid_joint_correction = &middle_correction;
    bool reached = false;
    job.reached = &reached;
    if (!job.Run()) {
        return BF_ANIMATION_STATUS_JOB_FAILED;
    }
    multiply_joint_rotation(
        configuration->start_joint,
        start_correction,
        ozz::make_span(pose->locals));
    multiply_joint_rotation(
        configuration->middle_joint,
        middle_correction,
        ozz::make_span(pose->locals));
    const int32_t status = update_models(skeleton, pose);
    *out_reached = reached ? 1U : 0U;
    return status;
}

extern "C" int32_t bf_animation_pose_copy_model_matrices(
    const BFAnimationPose *pose,
    BFAnimationMatrix *out_matrices,
    size_t matrix_count) {
    if (pose == nullptr || out_matrices == nullptr) {
        return BF_ANIMATION_STATUS_NULL_POINTER;
    }
    if (matrix_count != pose->models.size()) {
        return BF_ANIMATION_STATUS_INCOMPATIBLE;
    }
    for (size_t index = 0; index < matrix_count; ++index) {
        const auto &source = pose->models[index];
        ozz::math::StorePtrU(source.cols[0], out_matrices[index].columns + 0);
        ozz::math::StorePtrU(source.cols[1], out_matrices[index].columns + 4);
        ozz::math::StorePtrU(source.cols[2], out_matrices[index].columns + 8);
        ozz::math::StorePtrU(source.cols[3], out_matrices[index].columns + 12);
    }
    return BF_ANIMATION_STATUS_OK;
}
