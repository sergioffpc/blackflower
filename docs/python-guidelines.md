# Python engineering guidelines

## Scope and precedence

All project-owned Python code, including the offline cooker and integration
tests, follows the
[Google Python Style Guide](https://google.github.io/styleguide/pyguide.html).
Explicit project requirements take precedence; document necessary exceptions
next to the affected code or configuration. The cooker requires Python 3.12 or
newer, as declared in its [pyproject.toml](../tools/cooker/pyproject.toml).

## Development and review

-   Use four spaces for indentation and an 80-character line limit, with the
    guide's exceptions for imports, URLs and other unsplittable content.
    Separate top-level definitions with two blank lines and methods with one;
    use paragraph breaks between logical steps.
-   Import modules through absolute package paths. Import individual symbols
    only under the guide's exemptions, such as typing symbols. Group
    standard-library, third-party and project imports, sorting each group by
    full module path.
-   Use descriptive `snake_case` function and variable names, `CapWords` classes
    and `UPPER_CASE` constants. Preserve names required by external interfaces,
    such as `unittest.TestCase.setUp`.
-   Document public interfaces with a summary and applicable `Args`, `Returns`
    or `Yields`, `Raises`, and `Attributes` sections. Explain observable
    behavior and constraints; comments should explain decisions.
-   Annotate public function parameters and returns. Keep mutable defaults and
    unnecessary mutable global state out of interfaces. Prefer direct iteration
    and simple comprehensions; use ordinary loops when nesting obscures the
    operation.
-   Use context managers for files and similar resources. Catch specific
    exceptions and preserve exception causes when translating errors. Validate
    external input explicitly; assertions are not input validation.
-   Follow the [project error contract](style-guidelines.md#error-contracts):
    raise exception class instances, never return raw string/integer errors.
    Messages provide context; handlers distinguish types rather than text.
-   Test observable behavior within the scope defined by the
    [development process](development-process.md). Formatting and analysis do
    not establish behavioral correctness.

## Automated enforcement

| Tool                                               | Role                                                     | Configuration and limits                                                                                                                                                                                                |
| -------------------------------------------------- | -------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Pyink](https://github.com/google/pyink)           | Formatting, analogous to clang-format                    | `[tool.pyink]` selects 80 columns, four spaces and consistent quotes within each file. This Google-maintained Black fork is linked by the guide. It does not sort imports or infer logical paragraph breaks.            |
| [Pylint](https://pylint.readthedocs.io/en/stable/) | Static analysis and conventions, analogous to clang-tidy | Root [.pylintrc](../.pylintrc) adapts the [Google configuration](https://google.github.io/styleguide/pylintrc) for Pylint 4. Diagnostics fail the check.                                                                |
| [mypy](https://mypy.readthedocs.io/en/stable/)     | Static type checking                                     | `[tool.mypy]` checks unannotated function bodies. Missing third-party type information is tolerated; a successful run does not guarantee complete type coverage.                                                        |
| CTest                                              | Build enforcement                                        | The native `check` target runs `content.format`, `content.lint`, `content.typecheck`, and `content.pipeline`, alongside the C++ checks. CI installs the locked Python development environment before configuring CMake. |

Pyink and Pylint are development dependencies managed by uv; exact versions and
transitive dependencies are recorded in [uv.lock](../tools/cooker/uv.lock).
Extend check coverage when adding Python source directories and validate
configuration when upgrading tools.

The Pylint adaptation removes obsolete upstream options and checks, recognizes
`__main__`, discovers the cooker source root, uses one worker, enables
import-error checking, and rejects single-line conditionals. It retains other
upstream disabled checks, including member inference for dynamic libraries and
import ordering; these remain review responsibilities. Retain the upstream
Apache-2.0 attribution. Scope suppressions to the affected check and location,
with a reason. For example, exact integer validation intentionally rejects
booleans and integer subclasses.

Tools do not enforce the whole guide. Review module-only imports and their
order, naming meaning, docstring accuracy and completeness, interface design,
exception contracts, resource ownership, and logical grouping. A clean tool run
does not establish full Google conformance.

OpenUSD type information comes from the development-only `types-usd` package.
The pinned 24.5.2 stubs are unofficial and predate the 26.8 runtime; coverage is
partial. Validate the APIs used by the cooker with type checks and functional
tests when updating either dependency. Do not treat stub coverage as proof of
runtime compatibility.

## Commands

Run from the repository root, with
[uv installed](https://docs.astral.sh/uv/getting-started/installation/):

```sh
uv sync --locked --project tools/cooker
uv run --locked --no-sync --project tools/cooker pyink \
  --config tools/cooker/pyproject.toml tools/cooker/src tests/integration
uv run --locked --no-sync --project tools/cooker pyink --check \
  --config tools/cooker/pyproject.toml tools/cooker/src tests/integration
uv run --locked --no-sync --project tools/cooker pylint \
  --rcfile=.pylintrc tools/cooker/src/content \
  tests/integration/content_pipeline_test.py
uv run --locked --no-sync --project tools/cooker mypy \
  --config-file tools/cooker/pyproject.toml tools/cooker/src \
  tests/integration/content_pipeline_test.py
```

Then run `cmake --build --preset debug --target check` after following the
[build setup](build.md). To rerun Python checks against an already built runtime
harness, use `ctest --preset debug -R '^content\.'`. Changes are ready when
formatting, lint, type checks and affected tests pass, and the manual review
rules above have been considered.
