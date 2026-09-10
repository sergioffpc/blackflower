# Static analysis

Use clang-tidy with the repository [.clang-tidy](../.clang-tidy) for
project-owned C++23 code, tests, and benchmarks. The configuration is the
authoritative check selection. Pin clang-tidy with the Clang toolchain and
review changes to the enabled checks when upgrading it.

## Coverage and rationale

| Checks                                                 | Purpose in this project                                                                                                                                                      |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Clang Static Analyzer                                  | Find detectable invalid memory access, resource leaks, and broken execution paths.                                                                                           |
| Bug-prone constructs                                   | Catch lifetime mistakes, use after move, suspicious calculations, and implicit loss of numeric precision. Floating-point narrowing and template instantiations are included. |
| Concurrency                                            | Identify known unsafe threading patterns as parallel simulation work is introduced.                                                                                          |
| Performance                                            | Flag unnecessary copies, ineffective moves, and inefficient container operations for review.                                                                                 |
| Portability                                            | Expose dependencies on platform-specific constructs.                                                                                                                         |
| Selected Core Guidelines                               | Check initialization, global state, casts, object slicing, special member functions, and polymorphic destruction.                                                            |
| Selected Google, modernization, and readability checks | Support the adopted conventions, explicit ownership, modern special members, include hygiene, and readable declarations.                                                     |

Use selected Core Guidelines and modernization checks that support the
[C++ engineering guidelines](cpp-guidelines.md). Naming and design rules still
require review; the tool does not encode every recommendation. A clean analysis
result does not establish numerical fidelity, race freedom, bounded frame time,
or training effectiveness.

The `readability-function-size` check enforces the
[40-line function-body limit](cpp-guidelines.md#function-size), including blank
lines and comments. It applies to all project-owned C++ functions and methods in
the analysis scope.

## Running the analysis

Use the [reference Clang toolchain](build.md#prerequisites). The CMake presets
generate a compilation database and the check target runs configuration
verification, analysis, formatting, tests, and benchmark startup:

```sh
cmake --preset debug
cmake --build --preset debug --target check
```

The generated build/debug/compile_commands.json contains the actual compiler
options, target, include paths, and feature definitions. Use the corresponding
database for each build preset.

Validate the configuration with the pinned tool:

```sh
clang-tidy-21 --config-file=.clang-tidy --verify-config
clang-tidy-21 --config-file=.clang-tidy --list-checks
```

Analyze the bootstrap translation unit directly:

```sh
clang-tidy-21 -p build/debug --config-file=.clang-tidy src/main.cc
```

The analyze target derives its source list from the project target list in
CMakeLists.txt; the native check target depends on it. As targets are added,
extend analysis to every project-owned translation unit in the compilation
database, including tests and benchmarks. Reanalyze affected translation units
during development; header changes require analysis of their consumers. The
GitHub workflow preserves command failures and uploads check logs. Run analyze
with each Windows preset to check the corresponding target options and SDK
headers.

## Diagnostics and dependencies

Enabled warnings are errors. Resolve them before accepting a C++ change. The
header filter includes all non-system headers; mark external libraries such as
Boost as system includes in the build and exclude their translation units from
the project analysis target. Keep project headers visible. Define generated-code
exclusions narrowly when such code exists.

Review suggested fixes before applying them, especially those affecting
floating-point conversions, ownership, public interfaces, or allocation
behavior. An explicit cast alone does not establish that precision loss is
acceptable; use the project's numerical acceptance criteria.

For an intentional construct or a demonstrated false positive, use a suppression
limited to the specific check and code location, with an adjacent rationale and
relevant evidence. Prefer `NOLINTNEXTLINE(check-name)` over file-wide or
category-wide suppression. Changes to the check policy belong in the same review
as their justification.

Compiler warnings remain a separate build responsibility. Follow the
[development process](development-process.md) for tests, profiling,
reference-scene comparisons, and human evaluation.

## CodeQL security analysis

The [CodeQL workflow](../.github/workflows/codeql.yml) runs the
`security-extended` queries for C/C++ and GitHub Actions on Ubuntu 26.04. It
runs on pushes to main, develop, feature, release, and hotfix branches, and on
pull requests targeting main, develop, and release branches. It can also be
dispatched manually once the workflow exists on the default branch.

C/C++ analysis uses a manual Release build with the project's Clang 21, C++23,
CMake presets, and pinned vcpkg baseline. Dependencies are configured before
CodeQL initialization; the traced build explicitly selects `blackflower`,
`blackflower_content` and `blackflower_scene`, covering production code and its
compiled dependencies. Tests, the content harness and benchmarks are excluded
from CodeQL extraction at the owner's request. Keep this target list current
when production targets are added; a production dependency must not pull test
code into the build. The job sets `BLACKFLOWER_USE_SCCACHE=OFF` to invoke the
compiler directly for project targets: a cache hit or compilation delegated to
an existing sccache daemon would escape extraction. Dependency builds retain
their toolchain defaults and happen before tracing. GitHub Actions analysis uses
no build. The separate Linux CI remains responsible for clang-tidy, formatting,
tests, and sanitizers.

Inspect **Security → Code scanning** for findings and select the relevant
branch. Successful analysis means the scan completed, not that all findings have
been remediated. Review alerts before integration and record a reason for any
dismissal. CodeQL workflow checks are required by
[branch protection](git-workflow.md#protected-branches).

Advanced setup is intentional: the initial main baseline has no C++ source,
while develop contains the build infrastructure. Scanning starts on develop and
its PRs; main gains the workflow with the first release integration. No
scheduled scan is configured yet, because scheduled workflows run from the
default branch. Add a schedule when the workflow reaches main. Treat coverage as
production-target analysis for Linux Release, not an audit of every dependency,
Windows configuration, or possible execution path.

## References

-   [Clang-tidy usage and configuration](https://clang.llvm.org/extra/clang-tidy/).
-   [Available checks](https://clang.llvm.org/extra/clang-tidy/checks/list.html).
-   [Narrowing conversion checks](https://clang.llvm.org/extra/clang-tidy/checks/bugprone/narrowing-conversions.html).
-   [CodeQL advanced setup](https://docs.github.com/en/code-security/how-tos/find-and-fix-code-vulnerabilities/configure-code-scanning/configure-advanced-setup).
