#!/bin/sh

set -eu

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_directory/.." && pwd)

cd "$repository_root"

for package in \
    blackflower-world-simulation \
    blackflower-world-prediction
do
    dependencies=$(cargo tree --locked --edges normal --prefix none -p "$package")
    for forbidden in \
        blackflower-animation \
        blackflower-animation-format \
        blackflower-scripting \
        blackflower-scripting-luau
    do
        if printf '%s\n' "$dependencies" | grep -Eq "^${forbidden} v"; then
            printf '%s must not depend on %s\n' "$package" "$forbidden" >&2
            exit 1
        fi
    done
done

printf 'Simulation policy checks passed.\n'
