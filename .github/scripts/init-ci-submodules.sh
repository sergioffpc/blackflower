#!/bin/sh

set -eu

mode=${1:-}
package=${2:-}

initialize_slang()
{
    git submodule update --init --depth 1 \
        crates/blackflower-shader-compiler/vendor/slang
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
}

initialize_volume_cooker()
{
    git submodule update --init --depth 1 \
        crates/blackflower-cooker-volume/vendor/boost \
        crates/blackflower-cooker-volume/vendor/c-blosc \
        crates/blackflower-cooker-volume/vendor/oneTBB \
        crates/blackflower-rendering-volumes/vendor/openvdb
    git -C crates/blackflower-cooker-volume/vendor/boost \
        submodule update --init --depth 1 \
        libs/assert \
        libs/config \
        libs/container \
        libs/core \
        libs/detail \
        libs/function \
        libs/integer \
        libs/interprocess \
        libs/intrusive \
        libs/iostreams \
        libs/iterator \
        libs/move \
        libs/mpl \
        libs/numeric/conversion \
        libs/preprocessor \
        libs/random \
        libs/range \
        libs/regex \
        libs/smart_ptr \
        libs/static_assert \
        libs/throw_exception \
        libs/type_traits \
        libs/unordered \
        libs/utility \
        libs/winapi
}

case "$mode" in
    runtime)
        git submodule update --init --recursive --depth 1 \
            crates/blackflower-animation/vendor/ozz-animation \
            crates/blackflower-audio-spatial/vendor/flatbuffers \
            crates/blackflower-audio-spatial/vendor/embree \
            crates/blackflower-audio-spatial/vendor/libmysofa \
            crates/blackflower-audio-spatial/vendor/pffft \
            crates/blackflower-audio-spatial/vendor/steam-audio-sdk \
            crates/blackflower-audio-voice/vendor/opus \
            crates/blackflower-ecs/vendor/flecs \
            crates/blackflower-navigation/vendor/recastnavigation \
            crates/blackflower-physics/vendor/JoltPhysics \
            crates/blackflower-rendering-volumes/vendor/openvdb \
            vendor/zlib
        ;;
    assets)
        git submodule update --init --depth 1 \
            crates/blackflower-animation/vendor/ozz-animation \
            crates/blackflower-audio-voice/vendor/opus \
            crates/blackflower-navigation/vendor/recastnavigation \
            crates/blackflower-scripting/vendor/luau \
            crates/blackflower-rendering-textures/vendor/KTX-Software \
            vendor/zlib
        git submodule update --init --recursive --depth 1 \
            crates/blackflower-audio-spatial/vendor/flatbuffers \
            crates/blackflower-audio-spatial/vendor/embree \
            crates/blackflower-audio-spatial/vendor/libmysofa \
            crates/blackflower-audio-spatial/vendor/pffft \
            crates/blackflower-audio-spatial/vendor/steam-audio-sdk
        initialize_slang
        initialize_volume_cooker
        ;;
    native)
        case "$package" in
            blackflower-acoustics)
                :
                ;;
            blackflower-animation|blackflower-cooker-animation)
                git submodule update --init --recursive --depth 1 \
                    crates/blackflower-animation/vendor/ozz-animation
                ;;
            blackflower-audio-spatial)
                git submodule update --init --recursive --depth 1 \
                    crates/blackflower-audio-spatial/vendor/flatbuffers \
                    crates/blackflower-audio-spatial/vendor/embree \
                    crates/blackflower-audio-spatial/vendor/libmysofa \
                    crates/blackflower-audio-spatial/vendor/pffft \
                    crates/blackflower-audio-spatial/vendor/steam-audio-sdk \
                    vendor/zlib
                ;;
            blackflower-audio-capture|blackflower-audio-voice|blackflower-networking)
                git submodule update --init --recursive --depth 1 \
                    crates/blackflower-audio-voice/vendor/opus
                ;;
            blackflower-cooker-volume)
                git submodule update --init --depth 1 vendor/zlib
                initialize_volume_cooker
                ;;
            blackflower-ecs)
                git submodule update --init --recursive --depth 1 \
                    crates/blackflower-ecs/vendor/flecs
                ;;
            blackflower-navigation)
                git submodule update --init --recursive --depth 1 \
                    crates/blackflower-navigation/vendor/recastnavigation
                ;;
            blackflower-physics)
                git submodule update --init --recursive --depth 1 \
                    crates/blackflower-physics/vendor/JoltPhysics
                ;;
            blackflower-rendering-textures)
                git submodule update --init --depth 1 \
                    crates/blackflower-rendering-textures/vendor/KTX-Software
                ;;
            blackflower-rendering-volumes)
                git submodule update --init --depth 1 \
                    crates/blackflower-rendering-volumes/vendor/openvdb
                ;;
            blackflower-scripting)
                git submodule update --init --depth 1 \
                    crates/blackflower-scripting/vendor/luau
                ;;
            blackflower-shader-compiler)
                initialize_slang
                ;;
            *)
                printf 'Unsupported native package: %s\n' "$package" >&2
                exit 2
                ;;
        esac
        ;;
    *)
        printf 'Usage: %s runtime|assets|native <package>\n' "$0" >&2
        exit 2
        ;;
esac
