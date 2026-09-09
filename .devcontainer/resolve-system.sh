#!/bin/bash
# Explicit maintenance operation: resolve a new package lock in the base image.
set -euo pipefail

main() {
  local snapshot=${1:?Usage: resolve-system.sh YYYYMMDDTHHMMSSZ}
  [[ "$snapshot" =~ ^[0-9]{8}T[0-9]{6}Z$ ]]
  # Bootstrap HTTPS trust before the base image installs ca-certificates.
  local script_dir
  script_dir=$(dirname -- "$(readlink -f -- "${BASH_SOURCE[0]}")")
  local apt_options=(
    --snapshot "$snapshot"
    -o "Acquire::https::CaInfo=${script_dir}/snapshot-ca.pem"
  )
  local packages=(
    autoconf autoconf-archive automake bubblewrap build-essential
    ca-certificates
    clang-21 clangd-21 clang-format-21 clang-tidy-21 cmake curl gh git git-lfs
    gnupg jq libclang-rt-21-dev libtool lld-21 llvm-21 ninja-build
    openssh-client pkg-config python3.14 python3.14-venv sccache
    tar unzip xz-utils zip 7zip
  )
  apt-get "${apt_options[@]}" -o APT::Update::Error-Mode=any update >&2
  apt-get "${apt_options[@]}" -o Acquire::ForceHash=SHA256 \
    --print-uris --yes --download-only \
    --no-install-recommends install "${packages[@]}" \
    | while read -r url _filename _size digest; do
      if [[ "$digest" == SHA256:* ]]; then
        url=${url#\'}
        url=${url%\'}
        printf '%s %s\n' "${digest#SHA256:}" "$url"
      fi
    done | LC_ALL=C sort
}

main "$@"
