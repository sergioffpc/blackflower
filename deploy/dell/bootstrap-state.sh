#!/bin/bash
# Prepare a new operational checkout; review its signed commit before pushing.
set -euo pipefail

# Arguments: new checkout directory, private signing key, allowed signers file.
main() {
  local destination="${1:?New checkout path}" key="${2:?Signing key}"
  local allowed="${3:?Allowed signers file}" root name email remote
  root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
  name=$(git -C "$root" config user.name)
  email=$(git -C "$root" config user.email)
  remote=$(git -C "$root" remote get-url origin)
  [[ ! -e "$destination" && -f "$key" && -f "$allowed" ]]
  git init --initial-branch=gitops "$destination"
  git -C "$destination" remote add origin "$remote"
  git -C "$destination" config user.name "$name"
  git -C "$destination" config user.email "$email"
  git -C "$destination" config commit.gpgsign true
  git -C "$destination" config gpg.format ssh
  git -C "$destination" config user.signingkey "$key"
  git -C "$destination" config gpg.ssh.allowedSignersFile "$allowed"
  cp -R "${root}/deploy/dell/state/." "$destination/"
  git -C "$destination" add .
  git -C "$destination" commit -m 'chore(gitops): bootstrap Dell desired state'
  git -C "$destination" verify-commit HEAD
  printf 'Review %s before pushing the gitops branch.\n' "$destination"
}

main "$@"
