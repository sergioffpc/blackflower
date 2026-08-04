#!/bin/sh

set -eu

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_directory/.." && pwd)

cd "$repository_root"

legacy_modules=$(
    find . \
        \( -path './.git' -o -path './target' -o -path './vendor' \) \
        -prune -o -type f -name 'mod.rs' -print | sort
)

if [ -n "$legacy_modules" ]; then
    printf 'Rust modules must use the Rust 2024 file layout without mod.rs.\n' >&2
    printf '%s\n' "$legacy_modules" >&2
    exit 1
fi

printf 'Rust module layout is valid.\n'
