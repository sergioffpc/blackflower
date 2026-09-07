# Building Blackflower

The bootstrap builds a console executable, a GoogleTest harness, and a Google Benchmark harness. It uses CMake, Ninja, Clang 21, sccache, and vcpkg. Simulation behavior is not implemented yet.

## Prerequisites

The reference host is Ubuntu 26.04 x86-64. Install CMake 3.28 or newer, Ninja, Clang/LLVM 21, sccache, and vcpkg's host prerequisites:

```sh
sudo apt-get install cmake ninja-build sccache clang-21 clang-tidy-21 clang-format-21 \
  libclang-rt-21-dev llvm-21 build-essential git curl zip unzip tar pkg-config
```

The initial local validation baseline uses LLVM 21.1.8, libstdc++ 15.2.0, CMake 4.2.3, Ninja 1.13.2, and sccache 0.13.0. CI installs Ubuntu packages within the LLVM 21 series. This is a validation baseline, not a hermetic system image. Review toolchain upgrades together. The final simulator deployment platform remains undecided.

Visual Studio Code is the [reference editor](editor.md).

### vcpkg setup

Use [vcpkg](https://vcpkg.io/en/) in manifest mode. From the repository root:

```sh
git clone https://github.com/microsoft/vcpkg.git build/vcpkg
git -C build/vcpkg checkout --detach 04a9d8e5212d01ee1dd9478eadd9caade4f8b0d4
export VCPKG_ROOT="$PWD/build/vcpkg"
"$VCPKG_ROOT/bootstrap-vcpkg.sh" -disableMetrics
```

Keep VCPKG_ROOT exported in build shells and when launching the editor. A checkout outside the repository also works at this revision. The authoritative pin is builtin-baseline in [vcpkg.json](../vcpkg.json); CI reads it to select its vcpkg checkout. Review the manifest, these checkout instructions, and resolved versions together when updating the baseline.

Configuration installs GoogleTest 1.18.0 and Google Benchmark 1.9.5, plus the host helpers vcpkg-cmake and vcpkg-cmake-config. The baseline fixes their port versions and source hashes. These explicitly requested frameworks supply testing and measurement capabilities absent from the standard library. The gtest port also installs Google Mock; project targets do not link it. Add individual Boost components through the manifest when a concrete requirement needs them.

The triplets build static dependency libraries with Clang and the target's standard library. Project targets use C++23, warnings as errors, and disabled exceptions. Third-party ports retain their upstream language and exception settings; test frameworks are separate from the application binary and receive no throwing project callbacks. Sanitizer instrumentation currently covers project targets; third-party libraries are not instrumented, limiting detection within them.

The first installation requires network access, including tools acquired by vcpkg. Later builds reuse installed dependencies, vcpkg's binary cache, and sccache. vcpkg manages C++ dependencies; the compiler, editor, and top-level build tools are host prerequisites.

## Configure, build, and run

Run from the repository root with the tools on PATH and VCPKG_ROOT exported:

```sh
cmake --preset debug
cmake --build --preset debug
./build/debug/blackflower
```

| Preset | Target triplet | Configuration | Instrumentation |
| --- | --- | --- | --- |
| debug | x64-linux-clang | Debug | AddressSanitizer + UndefinedBehaviorSanitizer |
| tsan | x64-linux-clang | Debug | ThreadSanitizer |
| release | x64-linux-clang | Release | None |
| windows-debug | x64-windows-clang | Debug cross-build | AddressSanitizer |
| windows-release | x64-windows-clang | Release cross-build | None |

Each preset uses Ninja and its own build/<preset>/ directory. C++23 extensions and module scanning are disabled for project code. Missing required tools fail configuration. Use a fresh build directory when changing toolchains or triplets. Machine-specific preset overrides belong in the ignored CMakeUserPresets.json.

## Verification

Configure each preset before checking it:

```sh
cmake --build --preset debug --target check
cmake --preset tsan
cmake --build --preset tsan --target check
cmake --preset release
cmake --build --preset release --target check
```

The native check target builds all three executables, runs clang-tidy configuration verification, analyzes project sources using the compilation database, checks formatting, and runs CTest. It executes analysis even when compilation hits sccache. Any failed command fails the target. The analyze target performs build, analysis, and formatting without running tests and is available for cross-builds too.

CTest includes executable startup, an individually discovered GoogleTest build-contract test, and a brief benchmark harness run. These checks establish working framework integration and C++23 mode; they do not validate simulation behavior or performance. To run only tests after building, use `ctest --preset debug` or the matching native preset.

### Sanitizers

Every non-Release project target is instrumented. Linux Debug detects memory errors and undefined behavior; the separate tsan preset detects data races. ASan and TSan cannot be combined in one executable. Undefined-behavior diagnostics fail immediately, and sanitizer failures propagate through CTest. Linux ASan includes leak detection where supported. Keep debug symbols and frame pointers for diagnostic stacks.

Windows uses ASan; LLVM TSan does not support Windows. MSVC STL container annotations are disabled to match the uninstrumented vcpkg libraries, so accesses beyond a container's size but within its allocated capacity may escape detection. Run concurrency checks on Linux and retain target-specific execution tests for Windows. The cross-build produces instrumented binaries; detection occurs when those binaries run on Windows. Sanitizers only observe exercised paths and do not prove memory safety or race freedom. Release is always uninstrumented, including its benchmark harness.

## Windows cross-build from Linux

The x64-windows-clang toolchain uses Clang's GNU-style driver targeting x86_64-pc-windows-msvc, LLD, the Microsoft STL/CRT, and the Windows SDK. It does not require running the MSVC compiler. Install the extra host tools:

```sh
sudo apt-get install lld-21 llvm-21 7zip
```

Prepare SDK headers and libraries with [xwin 0.10.0](https://github.com/Jake-Shadle/xwin/releases/tag/0.10.0), using its x86_64 Linux binary. Run xwin from the repository root and review its Microsoft license prompt:

```sh
xwin --arch x86_64 --crt-version 14.44.35220 --sdk-version 10.0.26100 \
  --cache-dir build/xwin-cache splat --include-debug-libs --output build/windows-sdk
export XWIN_ROOT="$PWD/build/windows-sdk"
```

Retain the downloaded manifests for SDK reproducibility. The SDK's case-correcting symlinks are required on Linux. Debug CRT libraries are needed for Debug dependencies. Review the SDK and CRT together when upgrading; their version headers participate in vcpkg's package hash.

For Windows Debug ASan, extract the x86_64 ASan files from the official [LLVM 21.1.8 Windows distribution](https://github.com/llvm/llvm-project/releases/tag/llvmorg-21.1.8). On Linux, the Windows installer can be extracted without executing it:

```sh
7z x LLVM-21.1.8-win64.exe -obuild/llvm-windows 'lib/clang/21/lib/windows/*asan*x86_64*'
export LLVM_WINDOWS_ASAN_DIR="$PWD/build/llvm-windows/lib/clang/21/lib/windows"
```

Then build and analyze both configurations:

```sh
cmake --preset windows-debug
cmake --build --preset windows-debug --target analyze
cmake --preset windows-release
cmake --build --preset windows-release --target analyze
```

Outputs are blackflower.exe, blackflower_tests.exe, and blackflower_benchmarks.exe under the corresponding build directory. Debug configuration copies clang_rt.asan_dynamic-x86_64.dll beside them. Keep that DLL with the Debug executables. Windows must also provide the matching Microsoft runtime; Debug CRT files are development prerequisites, not redistributable release dependencies.

Cross-builds do not register runnable CTest cases and have no check target. On a prepared Windows machine, run all three executables, check their exit codes and sanitizer output, and use `blackflower_tests.exe --gtest_filter=...` when selecting tests. Record that execution evidence separately from cross-compilation. Windows is outside GitHub CI.

## Benchmarks

Use Release for measurements:

```sh
cmake --preset release
cmake --build --preset release
./build/release/blackflower_benchmarks \
  --benchmark_out=build/release/benchmarks.json --benchmark_out_format=json
```

FrameworkSmoke exercises the benchmark framework only. Add representative workloads when product code exists. Record revision, compiler, hardware, workload, and settings for performance claims. CI runs a short harness check without timing thresholds; hosted runner timings are not a stable performance baseline.

## Continuous integration

[Blackflower C++23 Build and Validation](../.github/workflows/ci.yml) runs only x64-linux-clang on GitHub's ubuntu-26.04 x86-64 runners. Its matrix checks debug, tsan, and release for pushes to Git-flow branches and pull requests targeting main, develop, or release branches. Manual dispatch is defined; GitHub exposes that control once the workflow exists on the default branch.

Each job installs host tools, checks out the manifest's vcpkg baseline, restores dependency and compiler caches, and runs the same configure and check commands as local development. Actions are pinned by commit. Compiler and package ABI hashes govern cache reuse. Jobs retain check logs, CTest evidence, and Release benchmark JSON for 14 days, including available evidence after failures. CI validates contributions; release publication follows the [Git workflow](git-workflow.md).

## Compiler cache

Ninja handles incremental builds. To observe sccache reuse, clean build outputs while retaining its cache, then rebuild:

```sh
cmake --build --preset debug --target clean
cmake --build --preset debug
sccache --show-stats
```

Set SCCACHE_DIR for a different writable cache location. Locally sccache uses its default cache; CI saves its cache directory through GitHub Actions caching.

## References

- [CMake presets](https://cmake.org/cmake/help/latest/manual/cmake-presets.7.html).
- [sccache](https://github.com/mozilla/sccache).
- [vcpkg CMake integration](https://learn.microsoft.com/en-us/vcpkg/users/buildsystems/cmake-integration).
- [vcpkg versioning](https://learn.microsoft.com/en-us/vcpkg/users/versioning).
- [GoogleTest CMake setup](https://google.github.io/googletest/quickstart-cmake.html).
- [Google Benchmark](https://github.com/google/benchmark).
- [GitHub runner images](https://github.com/actions/runner-images).
- [Clang AddressSanitizer](https://clang.llvm.org/docs/AddressSanitizer.html).
- [Clang ThreadSanitizer](https://clang.llvm.org/docs/ThreadSanitizer.html).
