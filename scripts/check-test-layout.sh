#!/bin/sh

set -eu

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_directory/.." && pwd)

cd "$repository_root"

rust_sources() {
    find . \
        \( -path './.git' -o -path './target' -o -path '*/vendor' -o -path '*/tests' \) \
        -prune -o -type f -name '*.rs' -print | sort
}

inline_tests=$(
    rust_sources | while IFS= read -r source; do
        awk '
            /^[[:space:]]*#\[(test|rstest|test_case|[[:alnum:]_]+::test)(\([^]]*\))?\][[:space:]]*$/ ||
            /^[[:space:]]*mod[[:space:]]+tests[[:space:]]*\{/ {
                printf "%s:%d:%s\n", FILENAME, FNR, $0
            }
        ' "$source"
    done
)

invalid_test_hooks=$(
    rust_sources | while IFS= read -r source; do
        awk '
            /^[[:space:]]*#\[cfg\([^]]*test[^]]*\)\][[:space:]]*$/ {
                configuration_line = FNR
                configuration = $0
                if ((getline path) <= 0 ||
                    path !~ /^[[:space:]]*#\[path = "(\.\.\/)+tests\/unit\/[^\"]+\.rs"\][[:space:]]*$/) {
                    printf "%s:%d:%s\n", FILENAME, configuration_line, configuration
                    next
                }
                if ((getline module) <= 0 ||
                    module !~ /^[[:space:]]*mod[[:space:]]+tests;[[:space:]]*$/) {
                    printf "%s:%d:%s\n", FILENAME, configuration_line, configuration
                }
            }
        ' "$source"
    done
)

if [ -n "$inline_tests" ] || [ -n "$invalid_test_hooks" ]; then
    printf 'Rust test implementations must live under a package tests/ directory.\n' >&2
    if [ -n "$inline_tests" ]; then
        printf '%s\n' "$inline_tests" >&2
    fi
    if [ -n "$invalid_test_hooks" ]; then
        printf '%s\n' "$invalid_test_hooks" >&2
    fi
    exit 1
fi

printf 'Rust test layout is valid.\n'
