# Project conventions

## Language

Write all project documentation in English, including Markdown files and commit message examples.

## Architecture and development

Use arc42 for architecture documentation and development. Before designing, implementing, or reviewing a change, read [the architecture document](docs/architecture.md) and follow the development workflow in section 8.

Update affected architecture sections in the same change as the implementation. Index significant decisions in section 9, record measurable quality scenarios in section 10, and track unresolved risks in section 11. Keep confirmed facts distinct from proposals and open questions; document only the detail needed to explain the system and its decisions.

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

## Agent skills

### Workflow

Follow the AI Hero engineering workflow when taking work from an idea through implementation and review. Read [docs/agents/workflow.md](docs/agents/workflow.md) before starting or resuming a stage.

### Issue tracker

Track specifications and implementation tickets in GitHub Issues. Before tracker operations, read [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md).

### Triage labels

Use the five default triage roles. Before classifying issues, read [docs/agents/triage-labels.md](docs/agents/triage-labels.md).

### Domain docs

Use a single context: root CONTEXT.md and docs/adr/. Before exploring the domain or changing its terminology or decisions, read [docs/agents/domain.md](docs/agents/domain.md).
