#!/bin/bash
# Install checksum-pinned Linux x86_64 style tools into the ignored build tree.
set -euo pipefail

# Download a release asset and verify it before installing or extracting it.
# Arguments: URL, destination path, expected SHA-256 digest.
# Returns: nonzero if the download or digest verification fails.
download() {
  local url="$1" destination="$2" digest="$3"
  curl --fail --location --silent --show-error "$url" -o "$destination"
  printf '%s  %s\n' "$digest" "$destination" | sha256sum --check --status
}

main() {
  if [[ "$(uname -s)" != Linux || "$(uname -m)" != x86_64 ]]; then
    printf 'This installer requires Linux x86_64.\n' >&2
    return 1
  fi
  local project_root install_dir download_dir
  project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
  install_dir="${project_root}/build/style/bin"
  download_dir=$(mktemp -d)
  # Expand this local value while it is in scope; %q preserves shell quoting.
  # shellcheck disable=SC2064
  trap "$(printf 'rm -rf -- %q' "$download_dir")" EXIT
  mkdir -p "$install_dir"

  local base='https://github.com'
  local release_url
  release_url="${base}/koalaman/shellcheck/releases/download/v0.11.0"
  download \
    "${release_url}/shellcheck-v0.11.0.linux.x86_64.tar.xz" \
    "${download_dir}/shellcheck.tar.xz" \
    '8c3be12b05d5c177a04c29e3c78ce89ac86f1595681cab149b65b97c4e227198'
  tar -xJf "${download_dir}/shellcheck.tar.xz" -C "$download_dir"
  install -m 755 "${download_dir}/shellcheck-v0.11.0/shellcheck" "$install_dir"

  download \
    "${base}/mvdan/sh/releases/download/v3.14.0/shfmt_v3.14.0_linux_amd64" \
    "${download_dir}/shfmt" \
    'fe42021c7272ef2d67ea36cbc3031683c625d0badec733ef3a57b567246a0b66'
  install -m 755 "${download_dir}/shfmt" "$install_dir"

  release_url="${base}/rhysd/actionlint/releases/download/v1.7.12"
  download \
    "${release_url}/actionlint_1.7.12_linux_amd64.tar.gz" \
    "${download_dir}/actionlint.tar.gz" \
    '8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8'
  tar -xzf "${download_dir}/actionlint.tar.gz" -C "$download_dir" actionlint
  install -m 755 "${download_dir}/actionlint" "$install_dir"
}

main "$@"
