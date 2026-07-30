#!/bin/sh

set -eu

mode=${1:-}

case "$mode" in
    runtime)
        git submodule update --init --recursive --depth 1 \
            crates/blackflower-animation/vendor/ozz-animation \
            crates/blackflower-audio-spatial/vendor/flatbuffers \
            crates/blackflower-audio-spatial/vendor/libmysofa \
            crates/blackflower-audio-spatial/vendor/pffft \
            crates/blackflower-audio-spatial/vendor/steam-audio-sdk \
            crates/blackflower-audio-spatial/vendor/zlib \
            crates/blackflower-audio-voice/vendor/opus \
            crates/blackflower-ecs/vendor/flecs \
            crates/blackflower-navigation/vendor/recastnavigation \
            crates/blackflower-physics/vendor/JoltPhysics \
            crates/blackflower-rendering-volumes/vendor/openvdb
        ;;
    assets)
        git submodule update --init --depth 1 \
            crates/blackflower-animation/vendor/ozz-animation \
            crates/blackflower-scripting/vendor/luau \
            crates/blackflower-shader-compiler/vendor/slang \
            crates/blackflower-rendering-textures/vendor/KTX-Software
        git -C crates/blackflower-shader-compiler/vendor/slang \
            submodule update --init --depth 1 \
            external/cmark \
            external/fast_float \
            external/lz4 \
            external/lua \
            external/miniz \
            external/spirv-headers \
            external/unordered_dense \
            external/vulkan
        ;;
    *)
        printf 'Usage: %s runtime|assets\n' "$0" >&2
        exit 2
        ;;
esac
