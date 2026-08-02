#!/bin/sh

set -eu

mode=${1:-}
package=${2:-}
profile=${3:-debug}
target=${4:-}

set -- build --profile "$profile"
if [ -n "$target" ]; then
    set -- "$@" --target "$target"
fi

case "$mode" in
    runtime|assets)
        :
        ;;
    native)
        case "$package" in
            blackflower-acoustics|blackflower-spatial-query)
                set -- "$@" embree
                ;;
            blackflower-animation|blackflower-cooker-animation)
                set -- "$@" ozz
                ;;
            blackflower-audio-capture|blackflower-networking)
                set -- "$@" steam-audio opus
                ;;
            blackflower-audio-voice)
                set -- "$@" opus
                ;;
            blackflower-audio-spatial)
                set -- "$@" steam-audio
                ;;
            blackflower-cooker-volume)
                set -- "$@" openvdb
                ;;
            blackflower-ecs)
                set -- "$@" flecs
                ;;
            blackflower-navigation|blackflower-cooker-navigation)
                set -- "$@" recast
                ;;
            blackflower-physics)
                set -- "$@" jolt
                ;;
            blackflower-rendering-textures)
                set -- "$@" ktx
                ;;
            blackflower-rendering-volumes)
                set -- "$@" openvdb
                ;;
            blackflower-scripting-luau)
                set -- "$@" luau
                ;;
            blackflower-shader-compiler)
                set -- "$@" slang
                ;;
            *)
                exit 0
                ;;
        esac
        ;;
    *)
        printf 'Usage: %s runtime|assets|native <package> [debug|release] [target]\n' "$0" >&2
        exit 2
        ;;
esac

cargo run --locked --package native -- "$@"
