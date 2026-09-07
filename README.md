# Blackflower

Blackflower is intended to be a first-person simulation for military mission training, prioritizing physical, acoustic, and visual realism. The project is in its initial setup phase; detailed requirements and implementation are still to be defined.

Architecture documentation and development follow [arc42](https://arc42.org/). Start with the [architecture document](docs/architecture.md), including the [development workflow](docs/architecture.md#8-crosscutting-concepts).

See [AGENTS.md](AGENTS.md) for project conventions.

The [development process](docs/development-process.md) defines Kanban, selected XP practices, and fidelity validation for a team of one or two people.

Implementation uses C++23 with Clang. The [C++ engineering guidelines](docs/cpp-guidelines.md) combine the Google C++ Style Guide, C++ Core Guidelines, and Effective C++ recommendations adapted to modern C++.

Use the standard library first and Boost for capabilities it does not provide. [Static analysis](docs/static-analysis.md) uses clang-tidy with checks for correctness, resource safety, numeric conversions, concurrency, and performance.
