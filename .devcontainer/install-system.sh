#!/bin/bash
# Install the locked system packages without live APT indexes.
set -euo pipefail

main() {
  local download_dir digest url script_dir
  script_dir=$(dirname -- "$(readlink -f -- "${BASH_SOURCE[0]}")")
  download_dir=$(mktemp -d)
  chown _apt:root "$download_dir"
  # Capture this local value while it remains in scope.
  # shellcheck disable=SC2064
  trap "$(printf 'rm -rf -- %q' "$download_dir")" EXIT
  while read -r digest url; do
    [[ "$digest" =~ ^[[:xdigit:]]{64}$ ]]
    [[ "$url" == https://snapshot.ubuntu.com/ubuntu/*/pool/*.deb ]]
    printf 'Downloading %s\n' "${url##*/}"
    /usr/lib/apt/apt-helper \
      -o "Acquire::https::CaInfo=${script_dir}/snapshot-ca.pem" \
      download-file "$url" \
      "${download_dir}/${url##*/}" "SHA256:${digest}" >/dev/null
  done </opt/blackflower/system-packages.lock

  # Only local, hash-verified files can satisfy installation. The base image
  # supplies already installed packages; no repository or indexes are consulted.
  rm -f /etc/apt/sources.list /etc/apt/sources.list.d/ubuntu.sources
  DEBIAN_FRONTEND=noninteractive apt-get \
    --no-install-recommends --yes install "$download_dir"/*.deb
}

main "$@"
