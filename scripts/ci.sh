#!/bin/sh

set -eu

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_directory/.." && pwd)
mode=${1:-all}

cd "$repository_root"

build_native_vendors() {
    printf 'Building shared native vendors...\n'
    cargo native build --profile debug
}

check_format() {
    printf 'Checking Rust formatting...\n'
    cargo fmt --all -- --check
}

check_module_layout() {
    printf 'Checking Rust module layout...\n'
    "$script_directory/check-rust-module-layout.sh"
}

check_test_layout() {
    printf 'Checking Rust test layout...\n'
    "$script_directory/check-test-layout.sh"
}

check_simulation_policy() {
    printf 'Checking simulation consistency policy...\n'
    "$script_directory/check-simulation-policy.sh"
}

run_clippy() {
    printf 'Running Clippy...\n'
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
}

run_tests() {
    printf 'Running tests...\n'
    cargo test --workspace --all-targets --all-features --locked

    printf 'Testing Blender metadata extension...\n'
    python3 -m unittest discover -s tools/blender/tests
    python3 tools/blender/build_blackflower_gltf_metadata.py
}

case "$mode" in
    all)
        build_native_vendors
        check_module_layout
        check_test_layout
        check_simulation_policy
        check_format
        run_clippy
        run_tests
        ;;
    format)
        check_module_layout
        check_test_layout
        check_simulation_policy
        check_format
        ;;
    *)
        printf 'Usage: %s [all|format]\n' "$0" >&2
        exit 2
        ;;
esac
