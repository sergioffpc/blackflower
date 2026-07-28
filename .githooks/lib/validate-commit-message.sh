#!/bin/sh

set -eu

message=$(git stripspace --strip-comments)
subject=$(printf '%s\n' "$message" | sed -n '1p')

case "$message" in
    *'
'*)
        cat >&2 <<'EOF'
Commit rejected: the message must contain exactly one line.

Use ! in the subject to identify a breaking change:
  feat(protocol)!: remove the legacy handshake
EOF
        exit 1
        ;;
esac

# Git creates merge subjects automatically. Keep them usable without asking the
# developer to rewrite Git-generated metadata.
case "$subject" in
    "Merge "*)
        exit 0
        ;;
esac

conventional_subject='^(build|chore|ci|docs|feat|fix|perf|refactor|revert|style|test)(\([a-z0-9][a-z0-9._/-]*\))?(!)?: [^[:space:]].*$'

if printf '%s\n' "$subject" | grep -Eq "$conventional_subject"; then
    exit 0
fi

cat >&2 <<'EOF'
Commit rejected: the subject must follow Conventional Commits.

Format:
  type(scope)!: description

Allowed types:
  build, chore, ci, docs, feat, fix, perf, refactor, revert, style, test

Examples:
  feat: add player movement
  fix(server): handle client disconnect
  feat(protocol)!: remove the legacy handshake
EOF

exit 1
