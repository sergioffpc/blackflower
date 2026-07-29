#ifndef BLACKFLOWER_ANIMATION_WRAPPER_H
#define BLACKFLOWER_ANIMATION_WRAPPER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BF_ANIMATION_STATUS_OK 0
#define BF_ANIMATION_STATUS_NULL_POINTER 1
#define BF_ANIMATION_STATUS_INVALID_ARGUMENT 2
#define BF_ANIMATION_STATUS_INVALID_ARCHIVE 3
#define BF_ANIMATION_STATUS_OUT_OF_MEMORY 4
#define BF_ANIMATION_STATUS_INCOMPATIBLE 5
#define BF_ANIMATION_STATUS_JOB_FAILED 6
#define BF_ANIMATION_STATUS_INDEX_OUT_OF_RANGE 7
#define BF_ANIMATION_STATUS_NATIVE_FAILURE 8

typedef struct BFAnimationSkeleton BFAnimationSkeleton;
typedef struct BFAnimationClip BFAnimationClip;
typedef struct BFAnimationSamplingContext BFAnimationSamplingContext;
typedef struct BFAnimationPose BFAnimationPose;

typedef struct BFAnimationVersion {
    uint32_t major;
    uint32_t minor;
    uint32_t patch;
} BFAnimationVersion;

typedef struct BFAnimationMatrix {
    float columns[16];
} BFAnimationMatrix;

BFAnimationVersion bf_animation_ozz_version(void);
const char *bf_animation_simd_implementation(void);

int32_t bf_animation_skeleton_load(
    const uint8_t *data,
    size_t size,
    BFAnimationSkeleton **out_skeleton);
void bf_animation_skeleton_destroy(BFAnimationSkeleton *skeleton);
uint32_t bf_animation_skeleton_joint_count(const BFAnimationSkeleton *skeleton);
int32_t bf_animation_skeleton_joint_parent(
    const BFAnimationSkeleton *skeleton,
    uint32_t joint,
    int32_t *out_parent);
int32_t bf_animation_skeleton_joint_name(
    const BFAnimationSkeleton *skeleton,
    uint32_t joint,
    const char **out_name,
    size_t *out_length);

int32_t bf_animation_clip_load(
    const uint8_t *data,
    size_t size,
    BFAnimationClip **out_clip);
void bf_animation_clip_destroy(BFAnimationClip *clip);
float bf_animation_clip_duration(const BFAnimationClip *clip);
uint32_t bf_animation_clip_track_count(const BFAnimationClip *clip);
int32_t bf_animation_clip_name(
    const BFAnimationClip *clip,
    const char **out_name,
    size_t *out_length);

int32_t bf_animation_sampling_context_create(
    uint32_t max_tracks,
    BFAnimationSamplingContext **out_context);
void bf_animation_sampling_context_destroy(BFAnimationSamplingContext *context);
uint32_t bf_animation_sampling_context_max_tracks(
    const BFAnimationSamplingContext *context);
void bf_animation_sampling_context_invalidate(
    BFAnimationSamplingContext *context);

int32_t bf_animation_pose_create(
    const BFAnimationSkeleton *skeleton,
    BFAnimationPose **out_pose);
void bf_animation_pose_destroy(BFAnimationPose *pose);
uint32_t bf_animation_pose_joint_count(const BFAnimationPose *pose);
int32_t bf_animation_pose_set_rest(
    const BFAnimationSkeleton *skeleton,
    BFAnimationPose *pose);
int32_t bf_animation_pose_sample(
    const BFAnimationSkeleton *skeleton,
    const BFAnimationClip *clip,
    BFAnimationSamplingContext *context,
    float ratio,
    BFAnimationPose *pose);
int32_t bf_animation_pose_copy_model_matrices(
    const BFAnimationPose *pose,
    BFAnimationMatrix *out_matrices,
    size_t matrix_count);

#ifdef __cplusplus
}
#endif

#endif
