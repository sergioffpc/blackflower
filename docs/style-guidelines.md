# Project style guidelines

## Scope and precedence

This is the entry point for C++, Python, JavaScript, Markdown, JSON, and shell
conventions. Language-specific engineering guides remain authoritative for their
detailed rules and adaptations.

Adopt the [Google Markdown][google-markdown], [JSON][google-json], and
[Shell][google-shell] style guides for all repository source files, including
vendored skills, templates, tests, hidden configuration, and Git hooks. Explicit
project requirements and the compatibility exceptions below take precedence.
Preserve third-party attribution when formatting vendored files.

The check discovers tracked and non-ignored untracked files through Git. It
includes Markdown, JSON/JSONC, JavaScript (`.js`, `.mjs`, `.cjs`), `.sh` and
`.bash` files, and extensionless scripts with Bash or sh shebangs. Ignored build
output and installed dependencies are outside source scope. Committed JSON
lockfiles remain checked. GitHub Actions shell steps are additionally checked
with actionlint and ShellCheck.

The XML filename of Google's JSON guide does not mean JSON with comments. It
describes JSON API conventions. The [tool research][research] records primary
sources and distinguishes each tool's coverage from review obligations.

[google-markdown]: https://google.github.io/styleguide/docguide/style.html
[google-json]: https://google.github.io/styleguide/jsoncstyleguide.xml
[google-shell]: https://google.github.io/styleguide/shellguide.html
[research]: research/google-style-enforcement.md

## Error contracts

Project-owned errors must be enums or class instances, never raw strings,
integers, numeric sentinels, or aliases of those primitives. Text is diagnostic
context, not error identity. Translate foreign status codes at integration
boundaries; operating-system process exit codes and required foreign ABIs are
boundary encodings, not internal error contracts. In Python and JavaScript,
raise or throw error/exception class instances and discriminate by type or typed
fields, not message text. Shell exit statuses remain at the process boundary.
Follow the [C++ typed-error rules](cpp-guidelines.md#typed-errors) for C++ APIs.

## C++ rules and enforcement

Use C++23 with Clang and follow the
[C++ engineering guidelines](cpp-guidelines.md). They combine Google style, the
C++ Core Guidelines, and Effective C++ with the project's precedence rules,
exception policy, and standard-library-first dependency policy. Use Boost for
required capabilities the standard library does not provide, subject to that
policy.

clang-format applies the repository [.clang-format](../.clang-format).
clang-tidy uses [.clang-tidy](../.clang-tidy) under the
[static-analysis policy](static-analysis.md); enabled warnings are errors. After
preparing the [build environment](build.md), run:

```shell
cmake --preset debug
cmake --build --preset debug --target check
```

The check includes formatting, analysis, and native tests. Review ownership,
lifetimes, interface design, concurrency, and numerical behavior separately;
automated checks do not establish these engineering properties completely.

## Python rules and enforcement

Follow the [Python engineering guidelines](python-guidelines.md), which adopt
the Google Python Style Guide and document project adaptations. Pyink formats
code; Pylint checks conventions and static diagnostics; mypy checks types. Their
configurations and locked development dependencies are maintained with the
offline cooker.

After [installing uv and the Python environment](python-guidelines.md#commands),
run the documented Pyink, Pylint, and mypy commands and the affected tests. The
native CMake `check` target includes `content.format`, `content.lint`,
`content.typecheck`, and `content.pipeline`; rerun them against an existing
build with:

```shell
ctest --preset debug -R '^content\.'
```

Review import conventions, naming, docstring accuracy, exception contracts,
resource ownership, and logical grouping separately. The shared style command
below also checks C++ and Python formatting across the whole source inventory.
It complements their static-analysis, type-checking, and behavior checks.

## JavaScript rules and enforcement

Follow the [Google JavaScript Style Guide][google-javascript] for `.js`, `.mjs`,
and `.cjs` source files. Use ES modules, `const` by default and `let` for
reassignment, two-space blocks, four-space continuations, single-quoted strings,
semicolons, and lowerCamelCase names. Use named exports, explicit relative
import extensions, and JSDoc for classes and declared functions. The 80-column
limit has the guide's exceptions for imports, exports, and URLs.

ESLint combines its recommended checks with the published Google rule options.
The [flat configuration](../tools/style/eslint.config.mjs) maps formatting rules
to ESLint Stylistic and replaces removed JSDoc rules with eslint-plugin-jsdoc in
Closure mode. Diagnostics fail the shared style check; the shared `format`
command runs ESLint `--fix` for JavaScript.

Node's explicit module extensions are a project adaptation to the guide's `.js`
filename rule. ESLint configuration keeps its required filename and default
export; other source uses named exports. Node globals are declared only for the
`tools/style` scripts. Declare an appropriate environment when adding browser or
other Node code rather than disabling undefined-variable checks globally.

Review module boundaries and import cycles, meaningful names, error handling,
JSDoc completeness and type accuracy, and the remaining language restrictions.
Google marks this JavaScript guide as no longer maintained; retain the adopted
contract and validate plugin compatibility when upgrading the tooling.

[google-javascript]: https://google.github.io/styleguide/jsguide.html

## Markdown rules

Document stable rules, contracts, decisions and essential rationale. State a
rule's full scope directly; avoid concrete implementation examples in general
policies. Keep technical specifics in the contracts or operational references
that require them. Do not transcribe conversations, intermediate choices or
incidental library comparisons. Link authoritative rules instead of repeating
them across documents.

Use a single H1 title, ATX headings, descriptive and distinct heading names,
sentence case, and blank lines around headings, lists, and fenced code blocks.
Give every fence a language, using `text` for plain output. Prefer Markdown
syntax and descriptive link text. Wrap ordinary prose at 80 columns; headings,
tables, code blocks, and unsplittable links have the upstream exceptions. Use
four spaces for nested list levels and continuation content. The project
formatter determines list markers, emphasis delimiters, and table alignment.

The [Prettier configuration](../.prettierrc.json) wraps prose and formats lists.
The [markdownlint configuration](../.markdownlint-cli2.jsonc) checks structure,
whitespace, line length, code fences, link syntax, and list conventions. Neither
tool evaluates whether the prose is correct, concise, useful, or in English.

Project adaptations for GitHub rendering:

-   Use repository-relative file links, including `../` between directories, so
    links work in local previews and GitHub branch views. Check their targets.
-   Use GitHub's heading outline instead of inserting unsupported `[TOC]` text.
-   Keep YAML frontmatter required by skills, with one H1 after it. Present
    document templates in labelled Markdown fences so example H1s and
    placeholder tags are rendered as examples.
-   Allow `details` and `summary` only when a collapsible section improves use.
    Ordinary prose, headings, and lists use Markdown. HTML comments may hold
    necessary metadata or a narrowly justified lint suppression.

Review heading meaning and capitalization, list continuation indentation,
document layout, table usefulness, link destinations and anchors, and the
accuracy and copyability of code examples. MD007 alone does not check every
ordered-list indentation case.

## JSON rules

Use strict JSON by default: quoted names and strings, valid JSON values, no
comments, no trailing commas, and unique object keys. Format with two spaces, LF
endings, and a final newline. Prettier formats; ESLint with `@eslint/json`
checks syntax, duplicate keys, and unsafe numeric values before formatting.
Formatting cannot substitute for syntax or consumer schema validation.

New project-defined properties use meaningful ASCII camelCase names; array
properties use plural names. Map keys represent data and follow their domain
contract. Review field meaning, types, nesting, optional-value omission, enum
values, and applicable reserved API fields against the Google guide. Apply API
envelope rules when designing an API, not to unrelated tool configuration.

Compatibility exceptions are explicit:

-   CMake presets, vcpkg manifests, npm manifests/lockfiles, style
    configuration, and the skill lockfile retain the keys required by their
    owning tools.
-   VS Code configuration under `.vscode/*.json` and files explicitly ending in
    `.jsonc` use JSONC parsing. This project still omits trailing commas.
-   Pack reference metadata retains the snake_case keys used by its
    [fixture consumer](../tests/fixtures/packs/README.md). Preserve those names
    and signed or canonical serialization contracts; a schema migration requires
    its own compatibility decision.

Schema validation belongs to the consumer's checks, such as CMake preset loading
and content pipeline validation. A clean style run does not prove every external
schema or the semantic naming rules.

## Shell rules

Use Bash for new scripts, two-space indentation, at most 80 columns, and
`$(...)` command substitution. Quote expansions, use arrays for argument lists,
use `[[ ... ]]` for Bash tests, separate `local` declarations from fallible
command substitutions, and report errors to standard error. Use lowercase
function and variable names; constants and exported environment variables use
uppercase. Give scripts a purpose comment, and document non-obvious function
arguments, globals, output, and failure behavior. Use a final `main "$@"` for
scripts with multiple functions and an executable entry point.

ShellCheck diagnostics at all severities fail validation. shfmt uses the
Google-recommended `-i 2 -ci -bn` flags and infers the dialect from the shebang.
The wrapper additionally enforces the 80-column limit for standalone scripts.
actionlint checks workflow syntax and applies ShellCheck to embedded shell;
review embedded shell formatting and examples in Markdown separately.

The existing commit-message hook retains `#!/bin/sh`, portable `[ ... ]`, and sh
syntax. The new style hooks require Bash, as does the shell-tool installer.
Vendored wizard templates retain public uppercase variables used by generated
stages and caller-provided values; local implementation variables follow the
normal naming rules. These compatibility exceptions do not relax syntax,
quoting, formatting, or diagnostic checks.

Review error propagation, pipeline status, function contracts, names, cleanup,
and whether the task is small enough for shell. Inline suppressions require a
specific diagnostic and adjacent rationale; do not disable a category to make a
failing check pass. Formatting does not establish behavioral correctness.

## Installation and commands

On Linux x86_64, use Node.js 22.13 or later in the 22.x line, or Node.js 24 or
newer; CI pins 22.22.1. Install npm, Bash, Git, curl, tar, xz, and coreutils.
Prepare clang-format 21 under the [build prerequisites](build.md#prerequisites)
and the locked [Python environment](python-guidelines.md#commands). From the
repository root:

```shell
npm ci --prefix tools/style --ignore-scripts
bash tools/style/install-shell-tools.sh
uv sync --locked --project tools/cooker
npm --prefix tools/style run check
git config --local core.hooksPath .githooks
```

The [npm lockfile](../tools/style/package-lock.json) fixes the JavaScript tools
and dependencies. Prettier 2.8.8 is deliberate: the tested 3.x versions changed
Markdown list padding away from the required four-column content indentation.
The [compatibility experiment](research/google-style-enforcement.md) records
this constraint; retain the list checks when evaluating an upgrade. The
[shell installer](../tools/style/install-shell-tools.sh) downloads versioned
ShellCheck, shfmt, and actionlint releases, verifies their SHA-256 digests, and
installs them under ignored `build/style/bin`. The checker adds that directory
to its tool search path. Other platforms can supply the same tool versions on
`PATH`; the installer itself targets Linux x86_64.

For a nonstandard Clang installation, `BLACKFLOWER_CLANG_FORMAT` can name the
clang-format 21 executable. Pyink runs from `tools/cooker/.venv/bin/pyink`.

To apply formatting, then verify the result:

```shell
npm --prefix tools/style run format
npm --prefix tools/style run check
npm --prefix tools/style test
```

Formatting may still exit unsuccessfully when a lint finding needs an edit.
Resolve the finding and rerun checks. Review the diff, preserving JSON values,
shell behavior, skill instructions, and code examples. Upgrade tools together
with their lockfile or download digests and rerun the full source inventory.

## Acceptance and CI

The [pre-commit hook](../.githooks/pre-commit) runs `check:staged`: it exports
the complete Git index to a temporary directory and checks that exact source and
its staged configuration. The hook checks all covered files in the proposed
commit, including unchanged files. It rejects missing tools or failed checks; it
never formats, stages, or changes working files. After a failure, format the
intended files, review the diff, and stage those changes explicitly.

The same gate runs for automatic merge commits and `git am` through
`pre-merge-commit` and `pre-applypatch`. Installed tool dependencies are reused
locally; committing does not download them. Hooks require the setup command in
each clone and can be bypassed locally with Git options, so the protected-branch
CI checks remain the integration gate. Keep commit signing enabled.

`npm --prefix tools/style test` uses real commits in a disposable repository.
For each of the six languages, it stages an invalid version while keeping a
correct working copy, verifies rejection, and checks that both versions remain
untouched. It also verifies that corrected source commits successfully.

The Linux Debug job runs the same full style check before the C++ build. A
failure prevents its existing required `Ubuntu 26.04 / debug` check passing;
this change does not require a new branch-protection status name.

Before accepting a change, require zero formatter differences, zero enabled lint
diagnostics, and successful affected behavior checks. Record manual review of
the semantic rules above. Exercise representative invalid JSON, malformed
Markdown, and unsafe shell when changing the enforcement configuration, so
accidentally disabled checks are detected. Tool success establishes the
automated subset of these standards, not complete semantic conformance.
