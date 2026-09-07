# Minimal content pipeline validation

Scope: #21, OpenUSD input, uv-managed Python and distinct `.bfclient` and
`.bfserver` extensions. The committed reference pack fixtures provide stable
identities; generated test private keys and runtime DLLs are not committed.

## Environment

Validation ran on 2026-09-07 in Ubuntu under WSL2, with actual Windows execution
through WSL interoperability on Windows 10.0.26200.9168. The tools were
Clang/LLVM 21.1.8, CMake 4.2.3, CPython 3.14.4 and uv 0.10.4. Python packages
follow `tools/cooker/uv.lock`; C++ ports follow the vcpkg manifest baseline and
committed libsodium overlay. The local vcpkg executable checkout was
04a9d8e5212d01ee1dd9478eadd9caade4f8b0d4; CI checks out the manifest baseline
itself. Windows uses CRT headers/libraries 14.44.35220 and SDK 10.0.26100 from
the documented xwin setup.

## Passing checks

-   Linux Debug, TSan and Release:
    `cmake --build --preset <preset> --target check`, eight CTest entries per
    preset, including ten Python integration methods, C++ ownership and
    build-contract tests, executable startup, benchmark startup, Pyink, Pylint
    and mypy. Clang-tidy and formatting pass.
-   Windows Debug and Release: cross-compilation and `analyze` pass.
-   Windows Release execution: ten Python-to-C++ integration methods, both C++
    tests, bootstrap and benchmark startup pass on the Windows host.
-   Git commit-message hook integration and actionlint pass.
-   Separate Standards and Spec reviews completed; all three observations were
    resolved. Regression coverage now rejects USD inherits, specializes,
    relationships and attribute connections before publishing any output.

The canonical artifact identities are in `tests/fixtures/packs/reference.json`.
Tests compare fixed expected bytes and IDs and exercise each pack alone, invalid
signed metadata/scene values, wrong trust/role/profile, tampering, truncation,
repeatability and failures of either signing, file production, verification and
pair publication.

## Windows Debug limitation

Windows Debug builds and analyzes successfully, but execution with the LLVM
21.1.8 ASan DLL and the available Microsoft Debug CRT aborts during CRT
initialization before the content harness reaches `main`. The report is
`bad-malloc_usable_size`, from `ucrtbased.dll` `recalloc_dbg` through
`register_onexit_function` while initializing `MSVCP140D.dll`. The same failure
occurs in the existing benchmark harness; the console bootstrap runs
successfully. No ASan errors are suppressed and no sanitizer is disabled for the
reported passing Linux checks. Windows Release provides the required actual
Windows content-consumption evidence; Windows Debug sanitizer runtime
compatibility remains an unresolved development-environment risk.

The Debug CRT was extracted locally from the Microsoft Visual Studio package
manifest: `Microsoft.VisualCpp.RuntimeDebug.14` x64, MSI SHA-256
4fd8cbe4ab8f1dc9b26f0e531ab0355780dc40ca0fd4f514e22de730c377ca38, CAB SHA-256
25e3a560327797998ac10357ed5ed41d5b638d36a9b092d2004fb912da027a43. The debug UCRT
came from the cached x64 SDK package. Release runtime DLLs were copied from the
host's installed Blender CRT directory for local execution. On the
case-sensitive WSL filesystem their names must match the PE imports, including
`MSVCP140.dll` and `VCRUNTIME140_1.dll`. Production redistribution is outside
this test harness delivery.

These results establish the bounded primitive content contract. They do not
establish gameplay startup, rendering, GPU/CPU physics, audio, admission,
fidelity or performance on the MVP reference machines.
