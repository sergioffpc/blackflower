# Static analysis

Use clang-tidy with the repository [.clang-tidy](../.clang-tidy) for project-owned C++23 code and tests. The configuration is the authoritative check selection. Pin clang-tidy with the Clang toolchain and review changes to the enabled checks when upgrading it.

## Coverage and rationale

| Checks | Purpose in this project |
| --- | --- |
| Clang Static Analyzer | Find detectable invalid memory access, resource leaks, and broken execution paths. |
| Bug-prone constructs | Catch lifetime mistakes, use after move, suspicious calculations, and implicit loss of numeric precision. Floating-point narrowing and template instantiations are included. |
| Concurrency | Identify known unsafe threading patterns as parallel simulation work is introduced. |
| Performance | Flag unnecessary copies, ineffective moves, and inefficient container operations for review. |
| Portability | Expose dependencies on platform-specific constructs. |
| Selected Core Guidelines | Check initialization, global state, casts, object slicing, special member functions, and polymorphic destruction. |
| Selected Google, modernization, and readability checks | Support the adopted conventions, explicit ownership, modern special members, include hygiene, and readable declarations. |

Use selected Core Guidelines and modernization checks that support the [C++ engineering guidelines](cpp-guidelines.md). Naming and design rules still require review; the tool does not encode every recommendation. A clean analysis result does not establish numerical fidelity, race freedom, bounded frame time, or training effectiveness.

## Running the analysis

The repository has no C++ source or build system yet. Execution becomes part of the build workflow when those are introduced; no source analysis has been performed as part of this setup.

Generate compile_commands.json from the actual Clang C++23 build, including its target, standard library, include paths, and feature definitions. The build directory is a command argument below, not a prescribed directory layout.

Validate the configuration with the pinned tool:

```sh
clang-tidy --config-file=.clang-tidy --verify-config
clang-tidy --config-file=.clang-tidy --list-checks
```

Analyze a translation unit, replacing the example paths with actual paths:

```sh
clang-tidy -p <build-directory> --config-file=.clang-tidy <source-file.cc>
```

When automation is added, run every project-owned translation unit in the compilation database for the acceptance check. Include test targets. Reanalyze affected translation units during development; header changes require analysis of their consumers. Preserve command failures and analysis logs in CI.

## Diagnostics and dependencies

Enabled warnings are errors. Resolve them before accepting a C++ change. The header filter includes all non-system headers; mark external libraries such as Boost as system includes in the build and exclude their translation units from the project analysis target. Keep project headers visible. Define generated-code exclusions narrowly when such code exists.

Review suggested fixes before applying them, especially those affecting floating-point conversions, ownership, public interfaces, or allocation behavior. An explicit cast alone does not establish that precision loss is acceptable; use the project's numerical acceptance criteria.

For an intentional construct or a demonstrated false positive, use a suppression limited to the specific check and code location, with an adjacent rationale and relevant evidence. Prefer `NOLINTNEXTLINE(check-name)` over file-wide or category-wide suppression. Changes to the check policy belong in the same review as their justification.

Compiler warnings remain a separate build responsibility. Follow the [development process](development-process.md) for tests, profiling, reference-scene comparisons, and human evaluation.

## References

- [Clang-tidy usage and configuration](https://clang.llvm.org/extra/clang-tidy/).
- [Available checks](https://clang.llvm.org/extra/clang-tidy/checks/list.html).
- [Narrowing conversion checks](https://clang.llvm.org/extra/clang-tidy/checks/bugprone/narrowing-conversions.html).
