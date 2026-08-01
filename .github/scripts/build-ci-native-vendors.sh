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
        set -- "$@" embree zlib
        ;;
    native)
        case "$package" in
            blackflower-acoustics|blackflower-spatial-query)
                set -- "$@" embree
                ;;
            blackflower-audio-capture|blackflower-audio-spatial|blackflower-networking)
                set -- "$@" embree zlib
                ;;
            blackflower-cooker-volume)
                set -- "$@" zlib
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

cargo run --locked --package blackflower-native-build -- "$@"
