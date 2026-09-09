#!/bin/bash
# Prepare locked project dependencies inside the development container.
set -euo pipefail

main() {
  if [[ "${BLACKFLOWER_DEVCONTAINER:-}" != 1 ]]; then
    printf 'Run this command inside the Blackflower Dev Container.\n' >&2
    return 1
  fi
  cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
  local baseline
  baseline=$(jq -er '."builtin-baseline"' vcpkg.json)
  if [[ "$(git -C "$VCPKG_ROOT" rev-parse HEAD)" != "$baseline" ]]; then
    printf 'The vcpkg baseline changed; rebuild the Dev Container.\n' >&2
    return 1
  fi
  mkdir -p "$VCPKG_DEFAULT_BINARY_CACHE" "$VCPKG_DOWNLOADS" "$SCCACHE_DIR"
  # Start the daemon before vcpkg can pass its filesystem locks to it.
  sccache clang++-21 --version
  uv sync --locked --project tools/content_pipeline
  uv run --locked --no-sync --project tools/content_pipeline \
    python -c 'from pxr import Usd; print("OpenUSD:", Usd.GetVersion())'
  npm ci --prefix tools/code_quality --ignore-scripts
  git config --local core.hooksPath .githooks

  if [[ "$#" == 0 ]]; then
    set -- debug
  fi
  local preset
  for preset in "$@"; do
    case "$preset" in
      debug | tsan | release) cmake --preset "$preset" ;;
      *)
        printf 'Unsupported native preset: %s\n' "$preset" >&2
        return 1
        ;;
    esac
  done
  if [[ -f build/debug/compile_commands.json ]]; then
    cp build/debug/compile_commands.json compile_commands.json
  fi
}

main "$@"
