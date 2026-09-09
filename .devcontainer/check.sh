#!/bin/bash
# Validate a fresh build; the caller must disable container networking.
set -euo pipefail

main() {
  if [[ "${BLACKFLOWER_DEVCONTAINER:-}" != 1 ]]; then
    printf 'Run this command inside the Blackflower Dev Container.\n' >&2
    return 1
  fi
  cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
  local preset="${1:-debug}"
  case "$preset" in
    debug | tsan | release) ;;
    *)
      printf 'Unsupported native preset: %s\n' "$preset" >&2
      return 1
      ;;
  esac
  # Explicit offline mode prevents Python downloads. Container network
  # isolation, applied by the caller, covers every build tool.
  export UV_OFFLINE=1
  sccache clang++-21 --version
  uv sync --locked --offline --project tools/content_pipeline
  if [[ "$preset" == debug ]]; then
    npm --prefix tools/code_quality run check
    npm --prefix tools/code_quality test
  fi
  local build_dir
  build_dir=$(mktemp -d "${PWD}/build/offline-${preset}-XXXXXX")
  # Keep this directory and its CTest logs as inspectable validation evidence.
  # Disable compiler and binary caches to exercise an actual offline rebuild.
  export SCCACHE_DISABLE=1
  export VCPKG_BINARY_SOURCES=clear
  cmake --preset "$preset" -B "$build_dir"
  cmake --build "$build_dir" --target check 2>&1 | tee "$build_dir/check.log"
  if [[ "$preset" == release ]]; then
    "${build_dir}/blackflower_benchmarks" --benchmark_min_time=0.001s \
      --benchmark_out="${build_dir}/benchmarks.json" \
      --benchmark_out_format=json
  fi
  printf 'Offline validation evidence: %s\n' "$build_dir"
}

main "$@"
