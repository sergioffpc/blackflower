# Runtime scene validation

Scope: [#61](https://github.com/sergioffpc/blackflower/issues/61), Phase 1 of
[#57](https://github.com/sergioffpc/blackflower/issues/57). The reviewed change
is based on local commit `3116941` from #58. This checkout's current feature
branch contains that unpublished prerequisite; fetching origin confirmed that no
remote branch contains it. Local reuse was explicitly requested by #61 and the
implementation request; neither issue closure nor remote integration is claimed
here.

## Acceptance boundary

The installed non-editable cooker CLI creates all three independently signed
packs from the referenced collision fixture. The actual C++ consumer
authenticates each pack with separate trust, prepares an immutable scene
resource and instantiates two copies in a real headless Flecs world. Assertions
use only public runtime operations, not private containers or mocks.

The test first failed because the runtime harness did not accept the instance
exercise. After implementation it passed for server, agent and client packs. The
coordinate migration first failed on the old world-space cap centre, then passed
after the cooker emitted local collision. The source/settings transcript retains
its legacy revision token; fixture schema versions remain 1.

| Observation                                      | Expected outcome                                                             |
| ------------------------------------------------ | ---------------------------------------------------------------------------- |
| Local cap centre                                 | `(1, 0, 0)` before placement.                                                |
| Original authored box placement                  | `(2, 1, 3)`, 90 degrees around Y, scale 2.                                   |
| Additional root placement                        | `(4, 3, -2)`, 90 degrees around Y, scale 2.                                  |
| Rooted cap centre / body dimensions              | `(6, 5, -6)` / `(4, 4, 4)` within documented geometric tolerances.           |
| Reused definition and independent live instances | Equal collider IDs/handles; separate instance namespaces and mutable poses.  |
| Invalid placement or derived overflow            | Typed rejection, no partial entities, existing instances preserved.          |
| Duplicate transfer destination                   | Typed rejection; destruction permits a subsequent transfer.                  |
| Transfer followed by original-owner unload       | Member survives with its new ownership, pose and collider lease.             |
| Final unload                                     | Zero managed entities; externally leased resources remain readable.          |
| Eviction, slot reuse and reload                  | Old resource generations and destroyed Entity handles are rejected.          |
| Foreign-world Entity / SceneInstance             | Typed stale-handle rejection.                                                |
| Renamed prim and relocated dependencies          | Authored identity preserved; relocated identical inputs produce equal bytes. |
| Independent reference fixtures                   | Fixed payload, AssetIds, ContentBuildId and signatures round-trip.           |

Positions/dimensions use 1e-9 metre and quaternion comparisons use 1e-12
component tolerance for these analytical fixtures. These are not physical
simulation accuracy guarantees. The scoped lifecycle/handle/invalid-transform
checks are the #57 exception to the usual minimal functional test policy;
concurrency, cancellation and SDK activation are absent and therefore untested.

## Commands and environment

Validation uses the development container, Clang 21.1.8 and the pinned
Python/vcpkg dependencies, including Flecs 4.1.6 and owner-selected GLM 1.0.3.
The commands below validate the implementation in this commit relative to
`3116941`; evidence is local to this checkout, not remote CI.

```sh
uv sync --locked --no-editable --project tools/content_pipeline \
  --reinstall-package cooker
cmake --preset debug
cmake --build --preset debug --target check
cmake --preset tsan
cmake --build --preset tsan --target check
cmake --preset release
cmake --build --preset release --target check
XWIN_ROOT="$PWD/build/windows-sdk" cmake --preset windows-release
XWIN_ROOT="$PWD/build/windows-sdk" \
  cmake --build --preset windows-release --target analyze
npm --prefix tools/code_quality run check
```

Linux Debug (ASan/UBSan), TSan and Release each passed the complete check
target: C++ formatting/clang-tidy and all ten CTest entries. The installed-CLI
integration suite contains sixteen methods, including the retained invalid-pack
matrix. Pyink, Pylint and mypy passed in each native configuration. No enabled
analysis or sanitizer diagnostics occurred. The shared source-style check
passed; relative Markdown link targets were also checked. These results
establish the headless fixture and its ownership contracts, not concurrency or
performance.

The initial Windows configuration attempt lacked XWIN_ROOT. The existing SDK can
be selected with `XWIN_ROOT="$PWD/build/windows-sdk"`; results must separate
cross-compilation from target execution. This container has no Windows
executable interoperability or wslpath. No Windows execution, reference-hardware
performance, PhysX behavior, GPU work or multi-world gameplay is claimed.

## Review

Independent Standards and Spec sub-agents reviewed the staged diff from
`3116941`, before the required signed commit. Standards found that global cache
collection on preparation failure could invalidate previously published handles.
A public-boundary regression failed on that behavior; removing the global
collection preserved earlier handles while releasing temporary leases. The test
now fails after preparing a valid first scene entity and overflowing a later
one, so it also checks that no prefix of a failed instance is published.

Spec found missing world-quaternion comparisons. The fixture now directly checks
the original world rotation against `(0, sqrt(0.5), 0, sqrt(0.5))` and the
rooted rotation against `(0, 1, 0, 0)` within 1e-12. The independent follow-up
reviews reported zero remaining findings on both axes. Manual review covered
lease ownership, instance namespaces, source identity, typed errors, GLM
ordering, Python imports, document links and the distinction between local and
delivered behavior.
