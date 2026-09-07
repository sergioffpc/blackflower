# Blackflower architecture

Status: initial baseline. This document follows the [arc42 structure](https://arc42.org/overview/). Open items identify missing information, not agreed requirements or implemented behavior.

## 1. Introduction and goals

Blackflower is intended to be a first-person simulation for military mission training, with physical, acoustic, and visual realism as product priorities. The repository contains a console bootstrap and build, test, and benchmark infrastructure. Simulation behavior has not been implemented.

The repository owner has requested arc42 adoption, English documentation, and signed Conventional Commits. Product stakeholders and their expectations remain to be identified.

Open: identify representative users, specific learning objectives, initial use cases, and measurable fidelity requirements before selecting a solution.

## 2. Architecture constraints

The repository is public on GitHub. Blackflower uses the [MIT License](../LICENSE); third-party materials retain their original notices. Contribution constraints are maintained in [AGENTS.md](../AGENTS.md).

Development capacity is one or two people. Implementation uses C++23 with Clang 21 under the [C++ engineering guidelines](cpp-guidelines.md). Build tooling uses CMake, Ninja, and sccache; vcpkg manages dependencies. Visual Studio Code is the [reference editor](editor.md). The build targets x64-linux-clang and cross-compiles x64-windows-clang from Linux; GitHub CI runs only the Linux target on Ubuntu 26.04. See the [build guide](build.md) for toolchain and platform prerequisites.

Open: select the simulation engine or libraries, target hardware, and deployment environment, and identify the remaining domain, organizational, technical, and operational constraints. Build target support does not select the final training platform.

Use the standard library first, with Boost as the preferred source for missing capabilities under the [library selection policy](cpp-guidelines.md#library-selection). GoogleTest and Google Benchmark provide the requested test and measurement frameworks through vcpkg. Static analysis uses clang-tidy under the [analysis policy](static-analysis.md). Concrete Boost components remain to be selected when required.

## 3. Context and scope

The business boundary, external actors, neighboring systems, and interfaces are not yet defined.

When the first use cases are agreed, describe the system boundary, responsibilities, exchanged information, and external interfaces here. Distinguish business interactions from technical connections.

## 4. Solution strategy

No product architecture has been selected. Derive the solution from the agreed use cases, constraints, and prioritized quality scenarios.

For each major approach, explain which goal it serves and link to the relevant decision in section 9. Treat unvalidated choices as proposals.

## 5. Building block view

| Building block | Responsibility |
| --- | --- |
| [Console bootstrap](../src/main.cc) | Print the project name and return a startup status. |
| [GoogleTest harness](../tests/build_test.cc) | Exercise test integration and the C++23 build contract. |
| [Google Benchmark harness](../benchmarks/framework_benchmark.cc) | Exercise benchmark registration and execution. |
| [Build configuration](../CMakeLists.txt) | Build the three independent executables and provide analysis and test checks. |

The frameworks are linked only into their respective harnesses. [Sanitizer configuration](../cmake/Sanitizers.cmake) instruments non-Release project targets for memory checks, with a separate Linux configuration for race detection. There are no simulation modules yet.

As implementation begins, document the main modules, their responsibilities, interfaces, and dependencies, with links to the source. Add detail where it helps explain important boundaries.

## 6. Runtime view

On startup, the bootstrap writes the project name to standard output and returns success unless the write reports failure. The test and benchmark executables run their framework checks separately. CTest coordinates native executable startup and framework checks; cross-build validation must distinguish compilation from execution on the target platform.

Document representative use cases and important failure paths as they are implemented, showing how the building blocks collaborate. Link scenarios to their requirements and verification evidence.

## 7. Deployment view

Local builds place outputs and dependency installations under build/. The [GitHub workflow](../.github/workflows/ci.yml) runs Linux Debug, ThreadSanitizer, and Release validation on Ubuntu 26.04 and retains diagnostic artifacts. Windows cross-builds are local build targets; they are outside CI. No application deployment or release publication pipeline exists yet.

When deployment is introduced, describe the environments, infrastructure, and mapping of software components to execution locations. Link to maintained deployment configuration and operational instructions.

## 8. Crosscutting concepts

### Development workflow

The [development process](development-process.md) defines Kanban, selected XP practices, and fidelity validation for the team. Follow its operating limits and completion criteria alongside the arc42 activities below.

Use the [build and dependency workflow](build.md), [reference editor setup](editor.md), and [Git-flow branch model](git-workflow.md) for implementation and integration.

GitHub enforces pull requests, signed commits, and required build and CodeQL checks on main and develop, including administrators. The [security policy](../SECURITY.md) defines confidential reporting for this public repository. Secret scanning and push protection supplement the [CodeQL and clang-tidy analysis](static-analysis.md). CodeQL uses a separate Linux Release build with compiler caching disabled to preserve extraction coverage; it does not replace sanitizer validation.

Apply the [arc42 method](https://arc42.org/method/) iteratively. The following activities inform one another; use them at the level of detail warranted by the change.

Use the [AI Hero agent workflow](agents/workflow.md) to move work through clarification, specification, tickets, implementation, and review. The activities below guide the architecture work within those stages. Tracker operations and domain documentation follow the [agent skills configuration](../AGENTS.md#agent-skills).

1. Clarify the intended behavior, relevant constraints, and acceptance criteria. For architectural work, establish the important quality goals and measurable scenarios before choosing a solution.
2. Design responsibilities and interfaces around the domain. Update the relevant context, building block, runtime, and deployment views.
3. Identify concepts shared across modules, such as error handling, persistence, security, and observability, when they become relevant. Explain how they support the quality goals.
4. Communicate significant choices and record their rationale in section 9. Keep open questions and proposals visible for feedback.
5. Implement in small increments and review the code against the documented responsibilities and decisions. Update affected documentation in the same change when implementation produces a better design.
6. Evaluate the result against acceptance criteria and quality scenarios using appropriate tests, measurements, or review. Record unresolved risks and use the findings to guide the next increment.

### Completion criteria

Apply the [development process completion criteria](development-process.md#completion-criteria), including acceptance checks, review evidence, current architecture documentation, and recorded risks.

Repository language and commit rules are defined in [AGENTS.md](../AGENTS.md).

## 9. Architectural decisions

No product architecture decisions have been made yet. arc42 adoption is an explicit project convention recorded in [AGENTS.md](../AGENTS.md).

Record significant future decisions in docs/adr/ following the [domain documentation rules](agents/domain.md), and index them here once created. Capture the status, context, driving requirements, alternatives considered, chosen approach, and consequences. When replacing a decision, retain its rationale and link to the replacement.

## 10. Quality requirements

Physical, acoustic, and visual realism are confirmed product priorities. Their measurable acceptance thresholds, reference data, and trade-offs with performance remain open. Training effectiveness also requires explicit learning objectives and evaluation criteria. The [validation process](development-process.md#fidelity-and-validation) describes how to gather this evidence; no fidelity or training outcomes have been validated yet.

For each agreed quality scenario, record a stable identifier, priority, stimulus and source, affected part of the system, operating conditions, expected response, and measurable acceptance threshold. Link it to the relevant design and validation evidence. Group scenarios by quality goal when useful.

## 11. Risks and technical debt

| ID | Open issue | Impact | Next step |
| --- | --- | --- | --- |
| R-001 | Specific users, learning objectives, and initial use cases are undefined. | Architecture choices could solve the wrong problem within the stated training purpose. | Establish these requirements with the repository owner before selecting the initial product architecture. |
| R-002 | Fidelity thresholds, reference data, target hardware, and operational constraints are undefined. | Technology and deployment choices cannot yet be evaluated against measurable product needs. | Agree prioritized quality scenarios and constraints before committing to those choices. |
| R-003 | The desired scope may exceed a one- or two-person team's capacity. | Work may expand faster than it can be integrated and validated. | Evaluate the small reference scene and observed delivery capacity before expanding the initial scope. |

The current checks validate build infrastructure only. They provide no evidence about simulation fidelity, performance budgets, or training outcomes. Update this section as implementation risks are discovered, mitigated, or resolved.

## 12. Glossary

No product domain terms have been agreed yet. When the first terms are resolved, create the root CONTEXT.md as the authoritative glossary and link it here, following the [domain documentation rules](agents/domain.md). Use its terminology consistently in requirements, documentation, and code.

## Template attribution

This document adapts the arc42 structure created by Gernot Starke and Peter Hruschka, licensed under [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). Template guidance has been replaced with the project's initial status and working conventions. See the [arc42 license information](https://arc42.org/license/).
