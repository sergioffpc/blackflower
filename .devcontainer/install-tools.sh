#!/bin/bash
# Install checksum-pinned tools that are outside the Ubuntu package lock.
set -euo pipefail

# Download and authenticate one archive before it can be extracted.
# Arguments: URL, output filename, expected SHA-256 digest.
download() {
  local url="$1" destination="$2" digest="$3"
  curl --fail --location --silent --show-error "$url" -o "$destination"
  printf '%s  %s\n' "$digest" "$destination" | sha256sum --check --status
}

main() {
  [[ "$(uname -m)" == x86_64 ]]
  local version
  version=$(cat /opt/blackflower/python-version)
  [[ "$(python3.14 --version)" == "Python ${version}" ]]

  local download_dir
  download_dir=$(mktemp -d)
  # Capture this local value while it remains in scope.
  # shellcheck disable=SC2064
  trap "$(printf 'rm -rf -- %q' "$download_dir")" EXIT

  local base='https://github.com/astral-sh/uv/releases/download/0.10.4'
  download "${base}/uv-x86_64-unknown-linux-gnu.tar.gz" \
    "${download_dir}/uv.tar.gz" \
    '6b52a47358deea1c5e173278bf46b2b489747a59ae31f2a4362ed5c6c1c269f7'
  tar -xzf "${download_dir}/uv.tar.gz" -C "$download_dir"
  install -m 755 "${download_dir}/uv-x86_64-unknown-linux-gnu/uv" \
    /usr/local/bin/uv

  base='https://nodejs.org/dist/v22.22.1'
  download "${base}/node-v22.22.1-linux-x64.tar.xz" \
    "${download_dir}/node.tar.xz" \
    '9a6bc82f9b491279147219f6a18add1e18424dce90d41d2a5fcd69d4924ba3aa'
  tar -xJf "${download_dir}/node.tar.xz" -C /usr/local --strip-components=1

  bash /opt/blackflower/tools/code_quality/install-shell-tools.sh
  gh --version
  dpkg-query -W >/opt/blackflower/system-packages.tsv
}

main "$@"
