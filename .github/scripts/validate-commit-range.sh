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
    subject=$(git show --no-patch --format=%s "$commit_oid")
    author_name=$(git show --no-patch --format=%an "$commit_oid")
    author_email=$(git show --no-patch --format=%ae "$commit_oid")

    is_dependabot_commit=false
    if [ "$author_name" = 'dependabot[bot]' ]
    then
        case "$author_email" in
            *'dependabot[bot]@users.noreply.github.com')
                is_dependabot_commit=true
                ;;
        esac
    fi

    if [ "$is_dependabot_commit" = true ]
    then
        case "$subject" in
            'chore(deps): '*)
                message=$subject
                ;;
            *)
                message=$(git show --no-patch --format=%B "$commit_oid")
                ;;
        esac
    else
        message=$(git show --no-patch --format=%B "$commit_oid")
    fi

    if ! printf '%s\n' "$message" |
        "$message_validator"
    then
        printf 'Invalid commit %s: %s\n' "$commit_oid" "$subject" >&2
        exit 1
    fi
done
