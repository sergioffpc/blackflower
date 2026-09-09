# Project conventions

## Language

Write all project documentation in English, including Markdown files and commit
message examples.

## C++ conventions

Implement the project in C++23 with Clang. Before designing, writing, or
reviewing C++ code, read [docs/cpp-guidelines.md](docs/cpp-guidelines.md) for
the Google C++ Style Guide, C++ Core Guidelines, Effective C++ recommendations,
and project-specific precedence and adaptations. Use the repository
[.clang-format](.clang-format) for formatting.

Use the standard library first and Boost for required capabilities it does not
provide, following the dependency policy in the C++ guidelines. Run clang-tidy
for C++ changes using [.clang-tidy](.clang-tidy) and the scope and acceptance
rules in [docs/static-analysis.md](docs/static-analysis.md).

## Python conventions

Follow the Google Python Style Guide for project-owned Python code. Before
designing, writing, or reviewing Python changes, read
[docs/python-guidelines.md](docs/python-guidelines.md) for the adopted rules,
tool configuration, validation commands, and review requirements. Run Pyink,
Pylint, mypy, and the affected tests for Python changes.

## Markdown, JSON, shell, and JavaScript conventions

Before writing or reviewing Markdown, JSON/JSONC, shell, or JavaScript, read
[docs/style-guidelines.md](docs/style-guidelines.md) for the adopted Google
guides, schema and portability exceptions, tool setup, and review requirements.
Run `npm --prefix tools/code_quality run check` for every change to these files.
All tracked and non-ignored source files, including skills and Git hooks, are in
scope; resolve diagnostics and review the rules tools cannot establish.

## Architecture and development

Use arc42 for architecture documentation and development. Before designing,
implementing, or reviewing a change, read
[the architecture document](docs/architecture.md) and follow the development
workflow in section 8.

Follow the Kanban, XP, and validation rules in
[docs/development-process.md](docs/development-process.md) when planning,
implementing, or reviewing work.

Update affected architecture sections in the same change as the implementation.
Index significant decisions in section 9, record measurable quality scenarios in
section 10, and track unresolved risks in section 11. Keep confirmed facts
distinct from proposals and open questions; document only the detail needed to
explain the system and its decisions.

## Commits

Follow [docs/git-workflow.md](docs/git-workflow.md) before creating branches,
committing, integrating changes, or preparing releases. Use feature branches
from develop for ordinary work; main receives releases and hotfixes.

All new commits must follow
[Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/).

Sign every commit. Keep `commit.gpgsign` enabled in this repository and verify
signatures with `git verify-commit` before pushing.

-   Format: `type: description` or `type(scope): description`.
-   Use `feat` for new features and `fix` for bug fixes.
-   For other changes, use an appropriate type such as `docs`, `refactor`,
    `test`, `chore`, `build`, `ci`, `perf`, or `style`.
-   Scope is optional; when present, identify the affected area in parentheses,
    for example `fix(api): correct validation`.
-   Mark breaking changes with `!` before `:` (for example
    `feat(api)!: remove legacy endpoint`) or a
    `BREAKING CHANGE: description of the incompatibility` footer.
-   Separate an optional body or footer section from the preceding section with
    a blank line.
-   Before creating a commit, verify that its message follows this convention
    and describes the included changes.

Examples: `feat: add authentication`, `fix: correct total calculation`,
`docs: update installation instructions`.

## Agent skills

### Workflow

Follow the AI Hero engineering workflow when taking work from an idea through
implementation and review. Read
[docs/agents/workflow.md](docs/agents/workflow.md) before starting or resuming a
stage. Use the committed skills in [.agents/skills](.agents/skills); read
[the setup guide](docs/agents/setup.md) when enabling or updating them.

### Issue tracker

Track specifications and implementation tickets in GitHub Issues. Before tracker
operations, read [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md).

### Triage labels

Use the five default triage roles. Before classifying issues, read
[docs/agents/triage-labels.md](docs/agents/triage-labels.md).

### Domain docs

Use a single context: root CONTEXT.md and docs/adr/. Before exploring the domain
or changing its terminology or decisions, read
[docs/agents/domain.md](docs/agents/domain.md).
