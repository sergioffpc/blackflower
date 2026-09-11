# Role-specific scene validation

Status: local functional evidence for
[#78](https://github.com/sergioffpc/blackflower/issues/78). This does not
establish runtime world construction, media loading, or SDK behavior.

## Boundary and fixture

The production-to-consumption boundary runs the installed Python cooker on
[`roles.usda`](../../tests/integration/fixtures/scenes/roles.usda), removes its
source tree, and loads all three signed outputs through the C++ content harness
with independently provisioned trust. The fixture contains collision-only,
visual-only, audio-only, and mixed entities.

The check observes exactly one `.bfserver`, `.bfagent`, and `.bfclient`; their
authenticated roles and common nonzero `ContentBuildId`; their decoded typed
domains; and stable `SceneEntityId` values. Server and Agent contain only the
two collider-bearing descriptions. Client contains all four descriptions, with
`SessionStatic` collision and logical visual/audio references where authored.

The loader rejects an authenticated Client pack when Server is required with
`PackError::kUnexpectedRole`. Existing malformed-pack checks cover signature,
digest, format, and scene failures. A signer failure while preparing the third
artifact leaves no output directory, lock, or visible staged file.

## Local checks

On 2026-09-11, the source state in this change passed these checks in the
development container:

-   `uv run --locked --no-sync --project tools/content_pipeline python tests/integration/content_pipeline_test.py`:
    all 20 integration methods passed.
-   `uv run --locked --no-sync --project tools/content_pipeline mypy ...` and
    the corresponding Pylint command from the Python guidelines: zero
    diagnostics.
-   `npm --prefix tools/code_quality run check`: all covered source files passed
    formatting and shared-language checks.
-   `cmake --build --preset debug --target check`: all 10 CTest entries passed,
    with Clang formatting and clang-tidy reporting no project diagnostics.
-   `cmake --preset tsan` followed by
    `cmake --build --preset tsan --target check`: all 10 CTest entries passed
    without sanitizer diagnostics.
-   `cmake --preset release` followed by
    `cmake --build --preset release --target check`: all 10 CTest entries
    passed.
-   With `XWIN_ROOT` set to the prepared SDK, `cmake --preset windows-release`
    followed by `cmake --build --preset windows-release --target analyze`:
    cross-compilation, formatting and clang-tidy passed.

## Limitations

Logical visual and audio references are identifiers only; the test does not load
media, render frames, or play audio. All authored collision in this bounded
subset is `SessionStatic`. Runtime derivation of Prediction and Presentation
worlds belongs to later tickets under the parent specification. Windows
execution and Windows Debug ASan were not available in this container; the
Windows result is compile/static-analysis evidence only.
