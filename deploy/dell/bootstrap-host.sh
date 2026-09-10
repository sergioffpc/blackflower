#!/bin/bash
# Bootstrap the confirmed Debian Dell host; refuses existing installations.
set -euo pipefail

# Download and verify a pinned upstream release before use.
download() {
  local url="$1" output="$2" digest="$3"
  curl -fLsS "$url" -o "$output"
  printf '%s  %s\n' "$digest" "$output" | sha256sum --check --status
}

main() {
  [[ "$EUID" == 0 ]]
  [[ "$(uname -m)" == x86_64 ]]
  if [[ -e /etc/rancher/k3s/config.yaml ||
    -e /etc/bind/named.conf.local ]]; then
    printf 'Existing installation: use the recovery instructions.\n' >&2
    return 1
  fi
  local root cache base candidates
  root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
  cache=$(mktemp -d)
  # Capture the private temporary directory for cleanup on exit.
  # shellcheck disable=SC2064
  trap "$(printf 'rm -rf -- %q' "$cache")" EXIT
  apt-get update
  # Prefer APT. Reassess any newly available package before binary fallback.
  candidates=$(apt-cache policy k3s)
  if [[ -n "$candidates" ]]; then
    printf 'APT candidates found; review package identity and versions.\n' >&2
    return 1
  fi
  base='https://github.com/k3s-io/k3s/releases/download/v1.36.4+k3s1'
  download "${base}/k3s" "${cache}/k3s" \
    '835873f37245fc615f547a2fe2af9402a347875f13fa64a1f136de644955ea3f'
  apt-get install -y --no-install-recommends \
    bind9 bind9-utils dnsutils nftables
  systemctl stop named
  install -d -m 755 /etc/blackflower /etc/rancher/k3s
  install -m 600 "${root}/host/k3s.yaml" /etc/rancher/k3s/config.yaml
  install -m 644 "${root}/host/firewall.nft" /etc/blackflower/firewall.nft
  install -m 644 "${root}/host/blackflower-firewall.service" \
    /etc/systemd/system/blackflower-firewall.service
  nft --check -f /etc/blackflower/firewall.nft
  systemctl daemon-reload
  systemctl enable --now blackflower-firewall
  install -m 755 "${cache}/k3s" /usr/local/bin/k3s
  install -m 644 "${root}/host/k3s.service" /etc/systemd/system/k3s.service
  install -m 644 "${root}/host/named.conf.options" /etc/bind/
  install -m 644 "${root}/host/named.conf.local" /etc/bind/
  install -o bind -g bind -m 640 \
    "${root}/host/blackflower.home.arpa.zone" /var/lib/bind/
  tsig-keygen -a hmac-sha256 blackflower-external-dns \
    >"${cache}/blackflower-tsig.key"
  install -o root -g bind -m 640 "${cache}/blackflower-tsig.key" /etc/bind/
  named-checkconf
  named-checkzone blackflower.home.arpa \
    /var/lib/bind/blackflower.home.arpa.zone
  systemctl daemon-reload
  systemctl enable --now named k3s
  printf 'Host installed. Wait for node readiness before Flux setup.\n'
}

main "$@"
