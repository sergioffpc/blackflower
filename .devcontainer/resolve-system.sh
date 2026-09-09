#!/bin/bash
# Explicit maintenance operation: resolve a new package lock in the base image.
set -euo pipefail

main() {
  local packages=(
    autoconf autoconf-archive automake bubblewrap build-essential
    ca-certificates
    clang-21 clangd-21 clang-format-21 clang-tidy-21 cmake curl git git-lfs
    gnupg jq libclang-rt-21-dev libtool lld-21 llvm-21 ninja-build
    openssh-client pkg-config python3.14 python3.14-venv sccache
    tar unzip xz-utils zip 7zip
  )
  apt-get -o APT::Update::Error-Mode=any update >&2
  apt-get -o Acquire::ForceHash=SHA256 --print-uris --yes --download-only \
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
