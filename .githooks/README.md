# Git hooks

Run the setup script once after cloning the repository:

```sh
./scripts/setup-git-hooks.sh
```

The `commit-msg` hook requires the entire commit message to contain exactly one
line in this form:

```text
type(scope)!: description
```

Commit bodies and footers are rejected. The scope and `!` are optional. The
accepted types are:

```text
build, chore, ci, docs, feat, fix, perf, refactor, revert, style, test
```

Examples:

```text
feat: add player movement
fix(server): handle client disconnect
feat(protocol)!: remove the legacy handshake
```

Use `!` in the subject for breaking changes because multiline
`BREAKING CHANGE:` footers are not accepted.

Git-generated merge commit subjects are accepted.

## Checks

The hooks run the following checks:

- `commit-msg`: validates one-line Conventional Commit messages.
- `pre-commit`: checks staged whitespace errors and Rust formatting.
- `pre-merge-commit`: runs the same checks as `pre-commit`.
- `pre-push`: validates outgoing commit messages, runs Clippy with warnings
  denied, and runs all workspace tests.

The pinned Rust toolchain uses the minimal profile and explicitly installs the
`rustfmt` and `clippy` components required by these hooks:

```toml
[toolchain]
channel = "1.97.1"
profile = "minimal"
components = ["clippy", "rustfmt"]
```
