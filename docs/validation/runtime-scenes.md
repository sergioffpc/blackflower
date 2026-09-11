# Runtime scene validation

Scope: [#61](https://github.com/sergioffpc/blackflower/issues/61), Phase 1 of
[#57](https://github.com/sergioffpc/blackflower/issues/57), including the
2026-09-11 single-Scene World amendment in
[ADR-0014](../adr/0014-instantiate-local-scene-entity-descriptions-in-ecs.md#single-scene-world-amendment).
Evidence is local to this checkout; no issue closure or remote integration is
claimed here.

## Acceptance boundary

The installed non-editable cooker CLI creates all three independently signed
packs from the referenced collision fixture. The actual C++ consumer
authenticates each pack with separate trust, prepares immutable resources and
loads one Scene into a real headless Flecs world. Assertions use only public
runtime operations, not private containers or mocks.

The single-Scene contract test first failed to compile against the former
`SceneInstance`, root-placement and transfer interface. It passed after
SceneWorld became the namespace and exposed Load, optional GetEntity by authored
identity, and whole-Scene Unload. The source/settings transcript retains its
legacy revision token; fixture schema versions remain 1.

| Observation                             | Expected outcome                                                               |
| --------------------------------------- | ------------------------------------------------------------------------------ |
| Local cap centre                        | `(1, 0, 0)` before placement.                                                  |
| Authored box placement                  | `(2, 1, 3)`, 90 degrees around Y, scale 2.                                     |
| World cap centre / body dimensions      | `(2, 1, 1)` / `(2, 2, 2)` within documented geometric tolerances.              |
| Second load                             | `kSceneAlreadyLoaded`; the active Scene and both entities remain unchanged.    |
| Entity update and invalid update        | Other members remain unchanged; invalid mutation preserves prior state.        |
| Destroy followed by GetEntity           | Lookup is empty and the member's old Entity handle is stale.                   |
| Whole-Scene unload                      | Zero managed entities; lookup is empty and repeated unload reports `kNoScene`. |
| Eviction, slot reuse and reload         | Old resource generations and destroyed Entity handles are rejected.            |
| Separate Worlds                         | Distinct entities; equal immutable collider handles; foreign handle rejected.  |
| Renamed prim and relocated dependencies | Authored identity preserved; relocated identical inputs produce equal bytes.   |
| Independent reference fixtures          | Fixed payload, AssetIds, ContentBuildId and signatures round-trip.             |

Metre values and quaternion components use a `1e-5` comparison tolerance for
these binary32 analytical fixtures. These are not physical simulation accuracy
guarantees. These scoped lifecycle, handle and invalid-transform checks are the
issue #57 exception to the usual minimal functional test policy; concurrency,
cancellation and SDK activation are absent and therefore untested.

## Commands and environment

Validation uses the development container, Clang 21.1.8 and the pinned
Python/vcpkg dependencies, including Flecs 4.1.6 and owner-selected GLM 1.0.3.

```sh
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

Linux Debug (ASan/UBSan), TSan and Release each passed all ten CTest entries.
The installed-CLI integration suite contains 21 methods, including the
single-Scene lifecycle exercise for Server, Agent and Client packs. Clang
format, clang-tidy, Pyink, Pylint, mypy and the shared source-style check passed
without diagnostics. Windows Release cross-compilation and analysis also passed.
Cross-compilation does not establish Windows execution. This container has no
Windows executable interoperability; no reference-hardware performance, PhysX
behavior, GPU work or multi-world gameplay is claimed.

## Historical review

The original #61 review found that global cache collection on preparation
failure could invalidate previously published handles; removing that collection
preserved earlier handles while temporary leases still released normally. It
also added explicit quaternion comparisons. Those resource-lifetime and geometry
findings remain applicable. The later single-Scene amendment removes the former
multi-instance ownership scenarios rather than preserving them as current
requirements.
