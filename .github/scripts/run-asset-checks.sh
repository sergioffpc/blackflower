#!/bin/sh

set -eu

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_directory/../.." && pwd)
check=${1:-}

cd "$repository_root"

set -- \
    --package blackflower-assets \
    --package blackflower-audio-spatial \
    --package blackflower-cooker-acoustics \
    --package blackflower-cooker-navigation \
    --package blackflower-cooker-volume \
    --package blackflower-gltf-metadata \
    --package blackflower-navigation \
    --package blackflower-rendering-models \
    --package blackflower-rendering-textures \
    --package blackflower-rendering-volumes \
    --package blackflower-scripting \
    --package blackflower-shader-compiler \
    --package xtask

case "$check" in
    clippy)
        cargo clippy "$@" \
            --all-targets --all-features --locked -- -D warnings
        ;;
    test)
        cargo test "$@" \
            --all-targets --all-features --locked --no-fail-fast
        python3 -m unittest discover -s tools/blender/tests
        python3 tools/blender/build_blackflower_gltf_metadata.py
        ;;
    doc)
        cargo test "$@" \
            --doc --all-features --locked
        ;;
    *)
        printf 'Usage: %s clippy|test|doc\n' "$0" >&2
        exit 2
        ;;
esac
