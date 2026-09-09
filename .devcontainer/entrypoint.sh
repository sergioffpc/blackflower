#!/bin/bash
# Prepare generated-volume ownership after Dev Containers remaps the user UID.
set -euo pipefail

main() {
  local directory uid
  uid=$(id -u ubuntu)
  for directory in \
    /workspaces/blackflower/build \
    /workspaces/blackflower/tools/content_pipeline/.venv \
    /workspaces/blackflower/tools/code_quality/node_modules \
    /home/ubuntu/.cache; do
    # Avoid scanning persistent caches on ordinary container starts.
    if [[ "$(stat -c %u "$directory")" != "$uid" ]]; then
      chown -R ubuntu:ubuntu "$directory"
    fi
  done
  exec "$@"
}

main "$@"
