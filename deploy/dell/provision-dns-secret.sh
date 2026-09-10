#!/bin/bash
# Copy the local DNS update key into Kubernetes without writing it to Git.
set -euo pipefail

main() {
  [[ "$EUID" == 0 ]]
  local secret_dir
  secret_dir=$(mktemp -d)
  # Capture the private directory; never emit the key or pass it in argv.
  # shellcheck disable=SC2064
  trap "$(printf 'rm -rf -- %q' "$secret_dir")" EXIT
  umask 077
  awk '/secret/ {gsub(/[";]/, "", $2); printf "%s", $2}' \
    /etc/bind/blackflower-tsig.key >"${secret_dir}/secret"
  [[ -s "${secret_dir}/secret" ]]
  k3s kubectl create namespace external-dns --dry-run=client -o yaml \
    | k3s kubectl apply -f -
  k3s kubectl -n external-dns create secret generic rfc2136 \
    --from-file="secret=${secret_dir}/secret" --dry-run=client -o yaml \
    | k3s kubectl apply -f -
}

main "$@"
