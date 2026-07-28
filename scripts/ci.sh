#!/bin/sh

set -eu

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_directory/.." && pwd)
mode=${1:-all}

cd "$repository_root"

check_format() {
    printf 'Checking Rust formatting...\n'
    cargo fmt --all -- --check
}

run_clippy() {
    printf 'Running Clippy...\n'
    cargo clippy --workspace --all-targets --locked -- -D warnings
}

run_tests() {
    printf 'Running tests...\n'
    cargo test --workspace --all-targets --locked
}

case "$mode" in
    all)
        check_format
        run_clippy
        run_tests
        ;;
    format)
        check_format
        ;;
    *)
        printf 'Usage: %s [all|format]\n' "$0" >&2
        exit 2
        ;;
esac
