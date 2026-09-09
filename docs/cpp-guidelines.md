# C++ engineering guidelines

## Language and toolchain

Use C++23 with Clang for project-owned application code and tests. The
[CMake build](build.md) selects Clang 21 and requires standard C++23 mode with
language extensions disabled.

Use the [reference toolchain](build.md#prerequisites) for validation. Verify the
features actually used against that compiler and library combination: selecting
a language mode does not guarantee complete language or library support. Consult
the [Clang C++ support table](https://clang.llvm.org/cxx_status.html).

The repository contains a minimal executable, GoogleTest and Google Benchmark
harnesses, compiler caching, sanitizers, and analysis checks.
[GitHub CI](build.md#continuous-integration) validates Linux on Ubuntu 26.04;
Windows cross-builds are verified locally. Visual Studio Code is the
[reference editor](editor.md).

Follow the shared [design principles](style-guidelines.md#design-principles) and
[public-interface rules](style-guidelines.md#public-interfaces).

## References and precedence

Apply these sources when designing, implementing, and reviewing C++ changes:

| Source                                                                              | Role                                                                                            |
| ----------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| [Google C++ Style Guide](https://google.github.io/styleguide/cppguide.html)         | Naming, formatting, headers, interfaces, and established language-use restrictions.             |
| [C++ Core Guidelines](https://isocpp.github.io/CppCoreGuidelines/CppCoreGuidelines) | Modern design, lifetime and resource safety, interfaces, concurrency, and performance guidance. |
| [Effective C++ third-edition notes](https://github.com/henrytien/effective-cpp-3rd) | Additional design and implementation recommendations, interpreted for C++23.                    |

Explicit project requirements and the adaptations below take precedence. Retain
the adopted Google style and restrictions where they conflict with general
recommendations; apply compatible Core Guidelines and Effective C++
recommendations together. Modern language rules and the Core Guidelines govern
how to replace obsolete mechanisms from the third-edition notes. For a remaining
conflict, identify the specific rules and document the resolution with the
change; use an ADR for a lasting architectural trade-off.

## Project adaptations

| Topic                        | Project interpretation                                                                                                                                                                                         |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Language version             | C++23 is the project baseline, overriding Google's C++20 ceiling. Other language-use restrictions still apply.                                                                                                 |
| Resource ownership           | Use RAII and standard resource-owning types. Prefer unique ownership; use shared ownership when the lifetime model requires it. Replace historical auto_ptr and TR1 examples with current standard facilities. |
| Special member functions     | Prefer the rule of zero. Use `= default` or `= delete` to express intent; define copy and move behavior consistently when a type requires custom resource management.                                          |
| Parameters and return values | Consider cost, ownership, and lifetime. Pass cheap values by value, borrow larger inputs appropriately, and return owned results by value when appropriate. Account for move semantics and copy elision.       |
| Errors                       | Confine dependency exceptions to isolated adapters that return typed errors. Keep other project code exception-free, following the Core Guidelines for systematic error reporting and resource cleanup.        |
| Headers                      | Follow Google's self-contained headers and direct-include rules. Apply recommendations about reducing compilation dependencies within those constraints.                                                       |
| Historical examples          | Use current standard-library facilities where suitable. A reference to TR1, Boost, or a support library in a guideline does not introduce that dependency into this project.                                   |

## Function organization

Within a module, group callers with the helpers they invoke and keep related
call paths together. Place shared helpers near their consumer group. Source
order does not guarantee instruction-cache locality; establish performance
effects from the compiled program and representative measurements.

## Aggregate initialization

Use designated initializers when supplying field values to aggregate structs.
Name each initialized field and follow declaration order; do not use positional
field values. Empty initialization for defaults remains valid.

## Function size

Function bodies must not exceed 40 lines after formatting, including blank lines
and comments. This applies to production code, tests and benchmarks. Extract
coherent responsibilities into named functions; do not compress statements,
remove useful documentation or introduce arbitrary forwarding layers to meet the
limit.

The `readability-function-size` clang-tidy check enforces a `LineThreshold`
of 40. Its body line span is the automated measure. Violations fail the existing
analysis check and must be resolved without suppressing this rule.

## Code documentation

Document the observable contract needed to use an interface without reading its
implementation. State purpose, caller obligations, effects and guarantees only
where names and types leave them unclear. Keep contracts at declarations and
implementation rationale beside the relevant code.

Error contracts must document each enum value's failure conditions and its
distinction from related values. Document the meaning, validity and availability
of every error field, including units and ownership or lifetime where relevant.
Names alone do not replace these contracts.

Use concise English comments to explain intent, non-obvious decisions or
necessary ordering. Do not narrate statements or repeat signatures. Improve
names and structure when they can express the information directly. Link to
authoritative schemas and architecture documents instead of copying their
contents. Simple, self-explanatory operations need no explanatory comments.

Update documentation with the behavior it describes. Review accuracy, ownership
and lifetime guarantees, terminology and duplication as well as formatting. The
[documentation guidance](research/software-documentation.md) provides the
supporting sources.

## Typed errors

Project-owned error contracts must use enums or classes. Use scoped enums for
closed failure categories and classes for structured error context. Prefer
`std::expected<T, E>` for fallible operations. Raw strings, integers, aliases
and numeric sentinels must not represent errors.

Keep error identity separate from diagnostic text. Handle failures by their
types or typed values, never by parsing messages. Translate foreign status codes
at integration boundaries; process exit codes and foreign ABI encodings stay at
those boundaries. Boolean predicates express conditions, not failure categories.

Project interfaces must not propagate exceptions. Enable exceptions only in
isolated dependency adapters that catch failures and translate them into typed
results before returning. Keep the rest of the project compiled without
exceptions. Check an expected result before accessing its value or error; do not
rely on throwing accessors. Apply the library-selection policy to error handling
as to any other capability. Review error contracts for semantic conformance;
automated tooling does not establish it.

## Library selection

Use the C++23 standard library first. When it does not provide a required
capability, use [Boost](https://www.boost.org/) as the preferred library source
before designing a custom implementation. Check the documentation for the
particular component and version.

Introduce only the components needed by a concrete requirement. Record the
standard-library gap, chosen component, pinned version, transitive dependencies,
and compatibility with the supported Clang and standard-library combination.
Evaluate allocation behavior, thread safety, error handling, and measured cost
where they affect the simulator's quality goals. If a component is unsuitable,
record the reason and chosen alternative.

If the standard specifies a facility but the selected standard-library
implementation lacks it, record that toolchain limitation and any temporary
Boost replacement. Review such replacements when the toolchain changes. Use
[vcpkg in manifest mode](build.md#vcpkg-setup) for C++ dependencies, with a
reviewed baseline and the project triplets. Add required Boost components
individually when a concrete gap is identified.

## Review focus

Consult relevant source sections and item numbers for each change. In
particular, examine:

-   Initialization, object invariants, const-correctness, and valid lifetimes
    for references and views.
-   Ownership transfer, destruction, copy and move semantics, and cleanup on
    failure.
-   Interfaces that express valid operations, encapsulation, and appropriate use
    of composition or polymorphism.
-   Shared mutable state, synchronization, and thread lifetimes when concurrency
    is introduced.
-   Compiler diagnostics and measured evidence for performance-sensitive
    choices.

Document the relevant rule or item when explaining a design choice or review
finding. Evaluate recommendations against the actual type and workload instead
of mechanically translating historical examples.

## Formatting and verification

Format code with the repository [.clang-format](../.clang-format), which uses
Google's preset. Use clang-tidy with [.clang-tidy](../.clang-tidy) and the
[static-analysis workflow](static-analysis.md). Run the native CMake check
target for formatting, analysis, tests, and benchmark startup, and the analyze
target for cross-builds; record scoped suppressions with their rationale.

Review requirements that automated tools cannot establish, and use the
[development process](development-process.md) for behavioral tests and fidelity
validation. Formatting success alone does not demonstrate conformance to the
engineering guidelines.
