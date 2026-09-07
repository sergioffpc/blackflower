# Project conventions

## Language

Write all project documentation in English, including Markdown files and commit message examples. Keep conversations with the user in Portuguese.

## Commits

All new commits must follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/).

Sign every commit. Keep `commit.gpgsign` enabled in this repository and verify signatures with `git verify-commit` before pushing.

- Format: `type: description` or `type(scope): description`.
- Use `feat` for new features and `fix` for bug fixes.
- For other changes, use an appropriate type such as `docs`, `refactor`, `test`, `chore`, `build`, `ci`, `perf`, or `style`.
- Scope is optional; when present, identify the affected area in parentheses, for example `fix(api): correct validation`.
- Mark breaking changes with `!` before `:` (for example `feat(api)!: remove legacy endpoint`) or a `BREAKING CHANGE: description of the incompatibility` footer.
- Separate an optional body or footer section from the preceding section with a blank line.
- Before creating a commit, verify that its message follows this convention and describes the included changes.

Examples: `feat: add authentication`, `fix: correct total calculation`, `docs: update installation instructions`.
