# Minimal content pipeline validation

Scope: #21, OpenUSD input, uv-managed Python and distinct `.bfclient` and
`.bfserver` extensions. The committed reference pack fixtures provide stable
identities; generated test private keys and runtime DLLs are not committed.

This record describes validation of an earlier revision. Current coverage is
defined by the
[test policy](../development-process.md#current-application-test-scope) and the
[pipeline guide](../content-pipeline.md#validation).

## Environment

Validation ran on 2026-09-07, with the Windows Debug correction revalidated on
2026-09-08, in Ubuntu under WSL2, with actual Windows execution through WSL
interoperability on Windows 10.0.26200.9168. The tools were Clang/LLVM 21.1.8,
CMake 4.2.3, CPython 3.14.4 and uv 0.10.4. Python packages follow
`tools/cooker/uv.lock`; C++ ports follow the vcpkg manifest baseline and
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
-   Windows Debug and Release execution: ten Python-to-C++ integration methods,
    both C++ tests, bootstrap and benchmark startup pass on the Windows host.
-   Git commit-message hook integration and actionlint pass.
-   Separate Standards and Spec reviews completed; all three observations were
    resolved. Regression coverage now rejects USD inherits, specializes,
    relationships and attribute connections before publishing any output.

The canonical artifact identities are in `tests/fixtures/packs/reference.json`.
Tests compare fixed expected bytes and IDs and exercise each pack alone, invalid
signed metadata/scene values, wrong trust/role/profile, tampering, truncation,
repeatability and failures of either signing, file production, verification and
pair publication.

## Windows Debug CRT correction

The original Windows Debug executables aborted before `main` with
`bad-malloc_usable_size`: `ucrtbased.dll` `recalloc_dbg` called ASan during
`MSVCP140D.dll` initialization. A minimal `iostream` program reproduced the
failure twice. Compiling the same program with the same LLVM 21.1.8 ASan
runtime, symbols and `-O0`, but selecting the release CRT, passed. This matches
the
[upstream Debug CRT incompatibility](https://github.com/google/sanitizers/wiki/AddressSanitizerWindowsPort#debug-crt-incompatibility).

The Windows toolchain now selects `MultiThreadedDLL` for all configurations,
including CMake dependencies; the libsodium Autotools overlay uses the same CRT.
GoogleTest, Benchmark and libsodium were rebuilt and their generated flags
checked for `msvcrt` without `_DEBUG`. Project Debug commands retain `-O0`,
symbols and `-fsanitize=address`, without `NDEBUG`. The Microsoft debug heap and
debug iterator ABI are not used.

The original startup probe now exits zero. A separate disposable probe, compiled
with the generated project Debug flags, allocates `new int[1]` and writes to
index one. It exits one with `AddressSanitizer: heap-buffer-overflow`,
confirming that memory checking remains active. No ASan diagnostic is
suppressed. The existing Windows integration and C++ tests exercise the original
failing startup path and pass with the corrected configuration.

Release runtime DLLs were copied from the host's installed Blender CRT directory
for local execution; Debug CRT DLLs are no longer needed. On the case-sensitive
WSL filesystem their names must match the PE imports, including `MSVCP140.dll`
and `VCRUNTIME140_1.dll`. Production redistribution is outside this test harness
delivery.

These results establish the bounded primitive content contract. They do not
establish gameplay startup, rendering, GPU/CPU physics, audio, admission,
fidelity or performance on the MVP reference machines.
