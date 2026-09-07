#!/bin/sh

# Exercise Git's real commit path in a disposable repository.
set -eu

project_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
test_repo=$(mktemp -d)
trap 'rm -rf "$test_repo"' EXIT HUP INT TERM
git init --quiet "$test_repo"
cd "$test_repo"
git config user.name 'Hook Test'
git config user.email 'hook-test@example.invalid'
git config commit.gpgsign false
git config core.hooksPath "$project_root/.githooks"

accept() {
  git commit --quiet --allow-empty -m "$1"
}

reject() {
  before=$(git rev-parse HEAD)
  if git commit --quiet --allow-empty -m "$1" >commit-output.log 2>&1; then
    printf 'Unexpectedly accepted: %s\n' "$1" >&2
    exit 1
  fi
  grep -q 'Commit rejected:' commit-output.log
  test "$(git rev-parse HEAD)" = "$before"
}

accept 'feat: add a simulation component'
accept 'fix(physics): correct collision handling'
accept 'feat(api)!: change initialization'
accept 'refactor!: remove obsolete setup'
accept 'build(deps): update the baseline'
accept "$(printf 'docs: explain setup\n\nAdditional context.')"
accept "Merge branch 'feature/example' into develop"
accept 'Merge pull request #1 from example/feature'
reject 'update files'
reject 'feat:add a component'
reject 'feat: '
reject 'feat(): add a component'
reject 'Merge '
reject "$(printf 'invalid subject\n\nfeat: valid body does not repair the subject')"

# Confirm that Git-generated merge messages also reach and pass the hook.
base_branch=$(git branch --show-current)
git checkout --quiet -b feature/merge-probe
accept 'test: add feature history'
git checkout --quiet "$base_branch"
accept 'test: add base history'
GIT_MERGE_AUTOEDIT=no git merge --quiet --no-ff feature/merge-probe

printf 'Commit message hook integration checks passed.\n'
