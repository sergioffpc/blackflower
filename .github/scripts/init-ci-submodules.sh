#!/bin/sh

set -eu

mode=${1:-}
package=${2:-}

initialize_slang()
{
    git submodule update --init --depth 1 \
        vendor/slang
    git -C vendor/slang \
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
        vendor/boost \
        vendor/c-blosc \
        vendor/oneTBB \
        vendor/openvdb
    git -C vendor/boost \
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

initialize_spatial_audio()
{
    git submodule update --init --recursive --depth 1 \
        vendor/flatbuffers \
        vendor/libmysofa \
        vendor/pffft \
        vendor/steam-audio-sdk \
        vendor/embree \
        vendor/zlib
}

initialize_all()
{
    git submodule update --init --recursive --depth 1 \
        vendor/JoltPhysics \
        vendor/KTX-Software \
        vendor/embree \
        vendor/flatbuffers \
        vendor/flecs \
        vendor/libmysofa \
        vendor/luau \
        vendor/opus \
        vendor/ozz-animation \
        vendor/pffft \
        vendor/recastnavigation \
        vendor/steam-audio-sdk \
        vendor/zlib
    initialize_slang
    initialize_volume_cooker
}

case "$mode" in
    runtime)
        initialize_all
        ;;
    assets)
        initialize_all
        ;;
    native)
        case "$package" in
            blackflower-acoustics)
                git submodule update --init --recursive --depth 1 vendor/embree
                ;;
            blackflower-animation|blackflower-cooker-animation)
                git submodule update --init --recursive --depth 1 \
                    vendor/ozz-animation
                ;;
            blackflower-audio-spatial)
                initialize_spatial_audio
                ;;
            blackflower-audio-capture|blackflower-networking)
                git submodule update --init --recursive --depth 1 \
                    vendor/opus
                initialize_spatial_audio
                ;;
            blackflower-audio-voice)
                git submodule update --init --recursive --depth 1 \
                    vendor/opus
                ;;
            blackflower-cooker-volume|blackflower-rendering-volumes)
                git submodule update --init --depth 1 vendor/zlib
                initialize_volume_cooker
                ;;
            blackflower-ecs)
                git submodule update --init --recursive --depth 1 \
                    vendor/flecs
                ;;
            blackflower-navigation|blackflower-cooker-navigation)
                git submodule update --init --recursive --depth 1 \
                    vendor/recastnavigation
                ;;
            blackflower-physics)
                git submodule update --init --recursive --depth 1 \
                    vendor/JoltPhysics
                ;;
            blackflower-rendering-textures)
                git submodule update --init --depth 1 \
                    vendor/KTX-Software
                ;;
            blackflower-scripting-luau)
                git submodule update --init --depth 1 \
                    vendor/luau
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
