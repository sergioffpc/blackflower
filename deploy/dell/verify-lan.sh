#!/bin/bash
# Observe diagnostic DNS and UDP from a LAN client. No cluster access required.
set -euo pipefail

# Arguments: expected hostname (or "absent"), optional DNS server address.
main() {
  local expected="${1:?Expected diagnostic-v1, diagnostic-v2, or absent}"
  local name='diagnostic.blackflower.home.arpa' addresses reply status
  local resolver=()
  if [[ -n "${2:-}" ]]; then
    resolver=("@${2}")
  fi
  addresses=$(dig "${resolver[@]}" +short "$name" A)
  if [[ "$expected" == absent ]]; then
    status=$(dig "${resolver[@]}" +noall +comments "$name" A)
    [[ -z "$addresses" && "$status" == *"status: NXDOMAIN"* ]]
    printf 'PASS: diagnostic DNS record is absent.\n'
    return
  fi
  [[ "$addresses" == 192.168.86.20[0-9] ||
    "$addresses" == 192.168.86.2[12][0-9] ||
    "$addresses" == 192.168.86.23[0-9] ]]
  # The child Bash expands its own positional argument, not this shell.
  # shellcheck disable=SC2016
  reply=$(timeout 5 bash -c '
    exec 3<>/dev/udp/"$1"/27015
    printf hostname >&3
    dd bs=1024 count=1 <&3 2>/dev/null
  ' bash "$addresses")
  [[ "$reply" == "$expected" ]]
  printf 'PASS: %s (%s):27015 UDP returned %s\n' \
    "$name" "$addresses" "$reply"
}

main "$@"
