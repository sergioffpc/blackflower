#!/bin/sh

set -eu

base_oid=$1
head_oid=$2
script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
message_validator="$script_directory/../../.githooks/lib/validate-commit-message.sh"

case "$base_oid" in
    0000000000000000000000000000000000000000 | \
    0000000000000000000000000000000000000000000000000000000000000000)
        revision_range=$head_oid
        ;;
    *)
        revision_range="$base_oid..$head_oid"
        ;;
esac

git rev-list --reverse "$revision_range" |
while read -r commit_oid
do
    if ! git show --no-patch --format=%B "$commit_oid" |
        "$message_validator"
    then
        subject=$(git show --no-patch --format=%s "$commit_oid")
        printf 'Invalid commit %s: %s\n' "$commit_oid" "$subject" >&2
        exit 1
    fi
done
