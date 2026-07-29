#include "wrapper.h"

#include <ozz/animation/runtime/animation.h>
#include <ozz/animation/runtime/local_to_model_job.h>
#include <ozz/animation/runtime/sampling_job.h>
#include <ozz/animation/runtime/skeleton.h>
#include <ozz/base/containers/vector.h>
#include <ozz/base/io/archive.h>
#include <ozz/base/io/stream.h>
#include <ozz/base/maths/simd_math.h>
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
