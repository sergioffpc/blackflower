#!/bin/bash
# Identify every repository input used to construct the development image.
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
sha256sum \
  .dockerignore \
  .devcontainer/image-inputs.sh \
  .devcontainer/Dockerfile \
  .devcontainer/install-system.sh \
  .devcontainer/install-tools.sh \
  .devcontainer/system-packages.lock \
  .devcontainer/snapshot-ca.pem \
  .devcontainer/entrypoint.sh \
  tools/code_quality/install-shell-tools.sh \
  tools/content_pipeline/.python-version \
  vcpkg.json | sha256sum | cut -d ' ' -f 1
