# Google style enforcement research

Research date: 2026-09-07. This note records the evidence and selected tools for
C++, Python, JavaScript, Markdown, JSON, and shell enforcement. The
[central style guide](../style-guidelines.md) owns the project rules, commands,
and exceptions. Tool success establishes only the automated subset of each
guide; the local validation evidence is recorded below.

## C++ and Python enforcement

The [C++ guidelines](../cpp-guidelines.md) adopt Google style with the
documented C++23 adaptations, Core Guidelines, and Effective C++
recommendations. [clang-format][clang-format] supports Google's base style and
repository configuration; [.clang-format](../../.clang-format) selects 80
columns, two-space indentation, include grouping, and explicit formatting
adaptations. [clang-tidy][clang-tidy] provides selectable static checks;
[.clang-tidy](../../.clang-tidy) makes enabled diagnostics errors. The
[analysis policy](../static-analysis.md) defines scope and acceptance. Native
`cmake --build --preset debug --target check` combines these checks with tests;
ownership, lifetime, architecture, and other semantic requirements need review.
The shared style runner now additionally applies
`clang-format-21 --dry-run --Werror` to every inventoried C/C++ source and
header. This formatting gate complements the build-dependent analysis and tests.

The [Python guidelines](../python-guidelines.md) already adopt Google's Python
style. [Pyink][pyink] is Google's Black fork; the
[cooker configuration](../../tools/content_pipeline/pyproject.toml) selects 80
columns and four spaces. [Pylint][pylint] checks conventions and static
diagnostics using [.pylintrc](../../.pylintrc); [mypy][mypy] checks types with
untyped bodies included and missing dependency stubs tolerated. Exact
dependencies are in [uv.lock](../../tools/content_pipeline/uv.lock). Native
CTest runs `content.format`, `content.lint`, `content.typecheck`, and
`content.pipeline`. Import conventions, docstring accuracy, ownership, and
exception contracts still need review. The shared runner also invokes the
installed Pyink with the cooker configuration on every inventoried `.py` file,
extending formatting coverage beyond the cooker and integration-test
directories.

[clang-format]: https://clang.llvm.org/docs/ClangFormatStyleOptions.html
[clang-tidy]: https://clang.llvm.org/extra/clang-tidy/
[pyink]: https://github.com/google/pyink
[pylint]: https://pylint.readthedocs.io/en/stable/user_guide/usage/run.html
[mypy]: https://mypy.readthedocs.io/en/stable/running_mypy.html

## JavaScript scope and enforcement

The [Google JavaScript guide][google-javascript] explicitly states that it is no
longer updated. Its adopted rules include two-space block indentation,
four-space continuation indentation, an 80-column limit with exceptions,
semicolons, single-quoted ordinary strings, `const`/`let`, ES modules, naming,
and JSDoc. Import and re-export statements are exempt from wrapping. The guide
permits default imports from nonconforming dependencies but bans default
exports.

[Google's ESLint configuration][eslint-google] is a useful rule baseline, but
its published 0.14.0 configuration needs adaptation for modern ESLint. [ESLint's
migration guide][eslint-migration] confirms removal of `require-jsdoc` and
`valid-jsdoc` and recommends `eslint-plugin-jsdoc` replacements. [ESLint
Stylistic][stylistic-migration] supplies the migrated formatting rules. The
[repository adapter](../../tools/code_quality/eslint.config.mjs) maps supported
Google rule names to that plugin and retains their options. It maps
`func-call-spacing` to `function-call-spacing` and deprecated `no-new-object` to
its documented successor, [`no-object-constructor`][eslint-object]. It adds
`@eslint/js` recommended checks for defects beyond the Google style baseline,
and adapts quote options and the ES import/export line-length exception.

[eslint-plugin-jsdoc][jsdoc-plugin] provides flat configurations and rules for
required documentation, parameter/return tags, and types. Its
[settings][jsdoc-settings] support Closure syntax, matching Google's type
annotations. The repository selects Closure mode and checks required JSDoc,
parameter/return tags, names, and types. These checks cannot establish accurate
descriptions, appropriate APIs, meaningful names, or freedom from module
dependency cycles; review remains necessary.

The project adaptations permit `.mjs` for Node ES modules and a default export
only in the ESLint configuration file. Node globals `process` and `console` are
scoped to `tools/code_quality/*.{js,mjs,cjs}`. The runner inventories `.js`,
`.mjs`, and `.cjs` files through the same Git source discovery and uses ESLint's
`--fix` in format mode. Remaining diagnostics fail validation; Prettier does not
format JavaScript. All four project `.mjs` files pass the completed local
validation recorded below.

[google-javascript]: https://google.github.io/styleguide/jsguide.html
[eslint-google]: https://github.com/google/eslint-config-google
[eslint-migration]: https://eslint.org/docs/latest/use/migrate-to-9.0.0
[stylistic-migration]: https://eslint.style/guide/migration
[eslint-object]: https://eslint.org/docs/latest/rules/no-new-object
[jsdoc-plugin]: https://github.com/gajus/eslint-plugin-jsdoc
[jsdoc-settings]:
    https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/settings.md

## JSON scope and enforcement

The [Google JSON guide][google-json] describes JSON API requests and responses.
Its XML filename does not define the JSON-with-comments format. It calls for
quoted names and strings, meaningful camelCase ASCII property names, and plural
array names. Map keys are explicitly exempt from property naming rules. API
envelopes, omission rules, and reserved names require design review; a formatter
cannot infer their semantics.

[ESLint's JSON plugin][eslint-json] provides separate JSON and JSONC languages.
The [repository configuration](../../tools/code_quality/eslint.config.mjs) uses
strict JSON by default, with explicit duplicate-key and unsafe-value errors.
Only `.jsonc` files and `.vscode/*.json` use JSONC; trailing commas remain
errors. The [runner](../../tools/code_quality/check.mjs) validates before
formatting, preventing a permissive formatter from silently repairing invalid
input.

Selected Prettier 2.8.8 formats strict files with `json-stringify` and comment
configurations with its `json` parser, as configured in
[.prettierrc.json](../../.prettierrc.json). Newer Prettier has a dedicated
`jsonc` parser, but it is unavailable in this selected version. The
[options][prettier-options] document permissive parsing; the [CLI][prettier-cli]
provides `--check` and `--write`. Formatting is separate from strict linting and
consumer schema validation.

Preserve external schemas. The [vcpkg manifest][vcpkg] defines keys such as
`version-string` and `default-features`; renaming them to camelCase breaks its
contract. [CMake presets][cmake] define their own properties and maps containing
cache-variable and environment names. [VS Code][vscode] defines dotted settings
and explicitly supports JSONC for `settings.json`, `tasks.json`, and
`launch.json`. These are schema-owned exceptions to project property naming, not
permission to relax project-owned API design.

[google-json]:
    https://raw.githubusercontent.com/google/styleguide/gh-pages/jsoncstyleguide.xml
[prettier-options]: https://prettier.io/docs/options
[prettier-cli]: https://prettier.io/docs/cli
[eslint-json]: https://github.com/eslint/json
[vcpkg]: https://learn.microsoft.com/en-us/vcpkg/reference/vcpkg-json
[cmake]: https://cmake.org/cmake/help/latest/manual/cmake-presets.7.html
[vscode]: https://code.visualstudio.com/docs/languages/json

## Markdown scope and enforcement

The [Google Markdown guide][google-markdown] calls for an 80-column prose limit
with exceptions for links, tables, headings, and code blocks; ATX headings and a
single H1; fenced, language-labelled code; no trailing spaces; and four-space
nested list indentation. Small, unnested, single-line lists can use one space
after markers. Its `[TOC]` instruction assumes hosting support, so GitHub-hosted
documentation needs an explicit project adaptation. Useful headings, link text,
document structure, and concise prose still need review.

[markdownlint rules][markdownlint-rules] cover headings, whitespace, fences,
languages, links, and list conventions. Configure MD007 with `indent: 4` and
MD013 with `line_length: 80`, `code_blocks: false`, `headings: false`, and
`tables: false`. Set MD009's `br_spaces` to zero. Limitations remain: MD007 only
checks unordered sublists whose ancestors are unordered. MD013 exempts reference
definitions and standalone links, but its long-line behavior is not identical to
Google's inline-link allowance. Ordered indentation and wrapped list text need
review or an additional rule; do not describe MD007 as enforcing all lists.

The [markdownlint Prettier guidance][markdownlint-prettier] describes four-space
indentation compatibility. Local comparison found that Prettier 3.9.6 and 3.6.2
with `tabWidth: 4` still produced one space after a dash and two-space wrapped
content. Prettier 2.8.8 produced three spaces after a dash and four-space
wrapped content. The
[selected dependency](../../tools/code_quality/package.json) therefore remains
2.8.8; a newer release is not automatically a compatible upgrade. The
[lint configuration](../../.markdownlint-cli2.jsonc) pairs it with MD030 marker
spacing of three for bullets and two for ordered lists. The
[CLI][markdownlint-cli] supports `--fix` for fixable violations. This formatting
experiment establishes the selected list behavior, not full guide compliance.

[google-markdown]: https://google.github.io/styleguide/docguide/style.html
[markdownlint-rules]:
    https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md
[markdownlint-prettier]:
    https://github.com/DavidAnson/markdownlint/blob/main/doc/Prettier.md
[markdownlint-cli]: https://github.com/DavidAnson/markdownlint-cli2

## Shell scope and enforcement

The [Google Shell guide][google-shell] recommends Bash and ShellCheck, two-space
indentation, 80-column lines, quoting, `$(...)`, and `[[ ... ]]`. Naming,
function documentation, error handling, and whether shell is appropriate remain
review responsibilities.

The [shfmt manual][shfmt] explicitly recommends `shfmt -i 2 -ci -bn` as closely
matching Google style. The repository runner adds `-d` for checks or `-w` for
formatting and preserves dialect detection for existing POSIX hooks.
[ShellCheck][shellcheck] runs with `--severity=style`, including all diagnostic
levels. The runner additionally rejects standalone shell lines over 80 columns;
shfmt does not establish line-length, naming, or documentation compliance.

[actionlint][actionlint] runs ShellCheck on Bash and sh workflow `run:` steps,
handling Actions expressions. Run `actionlint -shellcheck shellcheck`, checking
that ShellCheck is installed: default integration is skipped when unavailable.
It disables some ShellCheck diagnostics to accommodate workflow expressions. It
does not format embedded shell or replace shfmt. Prefer checked standalone
scripts for substantial shell logic; review remaining snippets for formatting.

[google-shell]: https://google.github.io/styleguide/shellguide.html
[shfmt]: https://github.com/mvdan/sh/blob/master/cmd/shfmt/shfmt.1.scd
[shellcheck]: https://github.com/koalaman/shellcheck
[actionlint]:
    https://github.com/rhysd/actionlint/blob/main/docs/checks.md#shellcheck-integration-for-run

## Selected versions and validation

The [npm manifest](../../tools/code_quality/package.json) and lockfile select
the JavaScript tools; supported Node.js versions are `^22.13.0 || >=24`. The
[installer](../../tools/code_quality/install-shell-tools.sh) selects the shell
tools and verifies release asset SHA-256 digests before installation.

| Tool                     | Selected version | Evidence                                                                               |
| ------------------------ | ---------------- | -------------------------------------------------------------------------------------- |
| Prettier                 | 2.8.8            | [Repository manifest](../../tools/code_quality/package.json) and list experiment above |
| ESLint / JSON plugin     | 10.0.0 / 2.1.0   | [Repository manifest](../../tools/code_quality/package.json)                           |
| @eslint/js               | 10.0.1           | [Repository manifest](../../tools/code_quality/package.json)                           |
| @stylistic/eslint-plugin | 5.10.0           | [Repository manifest](../../tools/code_quality/package.json)                           |
| eslint-config-google     | 0.14.0           | [Repository manifest](../../tools/code_quality/package.json)                           |
| eslint-plugin-jsdoc      | 63.0.0           | [Repository manifest](../../tools/code_quality/package.json)                           |
| markdownlint-cli2        | 0.23.2           | [Package manifest][markdownlint-package]                                               |
| shfmt                    | 3.14.0           | [Release][shfmt-release]                                                               |
| ShellCheck               | 0.11.0           | [Release][shellcheck-release]                                                          |
| actionlint               | 1.7.12           | [Release][actionlint-release]                                                          |

Run `npm --prefix tools/code_quality run check` after the installation commands
in the central guide. The runner inventories tracked and non-ignored untracked
source files, including vendored skills, hidden directories, and extensionless
shell hooks. Ignored build outputs and installed dependencies remain outside
scope. The [CI workflow](../../.github/workflows/ci.yml) pins Node.js 22.22.1
and runs this command after installing the native and Python tooling in the
Debug job, so a failure blocks the existing required `Ubuntu 26.04 / debug`
status.

## Commit enforcement evidence

The [staged checker](../../tools/code_quality/check-staged.mjs) exports the
complete Git index to a temporary directory, links installed tool dependencies,
and runs the snapshot's checker against its staged source and configuration. It
checks the proposed commit contents, including partially staged files, without
changing the working tree or index. The
[pre-commit hook](../../.githooks/pre-commit),
[merge hook](../../.githooks/pre-merge-commit), and
[git am hook](../../.githooks/pre-applypatch) invoke this gate.

The [commit integration test](../../tools/code_quality/staged.test.mjs), run by
`npm --prefix tools/code_quality test` locally and in CI, exercises real commits
in a disposable repository. It stages malformed examples for all six languages
and repairs only their working-tree copies to verify that validation reads the
index. It also exercises acceptance after corrected content is staged. The
[central guide](../style-guidelines.md) owns installation and acceptance policy.

Local validation on 2026-09-07 passed the full inventory: 7 C/C++ files, 7
Python files, 4 JavaScript modules, 88 Markdown files, 13 JSON/JSONC files, and
8 shell files, plus the GitHub Actions workflows. The JavaScript formatter was
run on all four `.mjs` files, followed by a check with zero diagnostics.

The commit integration test accepted formatted source and rejected staged
violations for all six languages while preserving both the staged and working
versions. Separate probes rejected JSON comments, trailing commas, duplicate
keys, and unsafe integers; formatting rejected invalid strict JSON without
rewriting it. The existing commit-message integration test also passed.

All 356 local Markdown file links resolved, and the 75 vendored file hashes were
refreshed and verified. CMake loaded the formatted presets, and clang-tidy
reported no project diagnostics for the benchmark's formatting-only change.
These are local results; the updated GitHub workflow has not been run remotely
as part of this change. Semantic conformance still requires the documented
review, beyond the automated subset.

[markdownlint-package]:
    https://raw.githubusercontent.com/DavidAnson/markdownlint-cli2/main/package.json
[shfmt-release]: https://github.com/mvdan/sh/releases/tag/v3.14.0
[shellcheck-release]:
    https://github.com/koalaman/shellcheck/releases/tag/v0.11.0
[actionlint-release]: https://github.com/rhysd/actionlint/releases/tag/v1.7.12
