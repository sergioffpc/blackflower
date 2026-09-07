# Blackflower

Blackflower is intended to be a first-person simulation platform for training in
virtual scenarios, with high physical, acoustic, and visual realism,
representing situations and environments that are difficult to reproduce in the
physical world. Each session will feature competition between an attacking Red
Team and a defending Blue Team, with up to eight participants per team.

The project currently has a minimal console executable and build setup. The
[first MVP specification](https://github.com/sergioffpc/blackflower/issues/11)
defines a four-player LAN experience with an authoritative Linux server and
Windows clients cross-compiled on Linux; simulation implementation and
validation remain pending.

Architecture documentation and development follow [arc42](https://arc42.org/).
Start with the [architecture document](docs/architecture.md), including the
[development workflow](docs/architecture.md#8-crosscutting-concepts).

The [selected technology stack](docs/technology-stack.md) and
[proposed C4 views](docs/c4.md) describe the intended MVP structure and its
remaining integration decisions.

See [AGENTS.md](AGENTS.md) for project conventions.

The [style guidelines](docs/style-guidelines.md) collect C++, Python,
JavaScript, Markdown, JSON, and shell rules, automated checks, and review
requirements.

The [development process](docs/development-process.md) defines Kanban, selected
XP practices, and fidelity validation for a team of one or two people.

Implementation uses C++23 with Clang. The
[C++ engineering guidelines](docs/cpp-guidelines.md) combine the Google C++
Style Guide, C++ Core Guidelines, and Effective C++ recommendations adapted to
modern C++.

Use the standard library first and Boost for capabilities it does not provide.
[Static analysis](docs/static-analysis.md) uses clang-tidy with checks for
correctness, resource safety, numeric conversions, concurrency, and performance.

## Build

With the [required tools and vcpkg checkout](docs/build.md#prerequisites)
prepared and VCPKG_ROOT exported:

```sh
cmake --preset debug
cmake --build --preset debug
cmake --build --preset debug --target check
./build/debug/blackflower
```

See [build instructions](docs/build.md) for GoogleTest, Google Benchmark,
sanitizers, compiler caching, and Windows cross-builds.
[GitHub CI](.github/workflows/ci.yml) validates Linux on Ubuntu 26.04. Visual
Studio Code is the [reference editor](docs/editor.md).

Development follows [Git-flow](docs/git-workflow.md), using feature branches
from develop. The AI Hero skills are
[installed in the repository](docs/agents/setup.md).

## License

Blackflower is licensed under the [MIT License](LICENSE). Third-party materials
retain their original licenses and notices, including the
[AI Hero skills](.agents/skills/AIHERO-LICENSE) and the
[arc42 template attribution](docs/architecture.md#template-attribution).
