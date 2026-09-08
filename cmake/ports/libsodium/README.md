# libsodium overlay

Based on the pinned vcpkg libsodium 1.0.22#1 port, Git tree
95eb2b61a8632cbfe65fd3cf259f8805b8792364. The copied vcpkg build metadata
retains its [Microsoft MIT license](LICENSE); upstream source hashes and patches
are unchanged.

The overlay restricts the unused vcpkg-msbuild host dependency to native Windows
builds. The upstream recipe already selects Autotools for Clang targeting
Windows; requiring the Windows-only helper on Linux prevented resolution before
that path could run.

For the non-MSVC Windows path, it supplies explicit LLD selection and dynamic
MSVC CRT flags after vcpkg-make escapes SDK paths. CMake's implicit runtime and
linker choices otherwise do not reach Autotools. Both configurations use the
release CRT to match the Windows toolchain and avoid the Debug CRT startup
failure under LLVM ASan. Debug optimization and symbols remain unchanged. Linux
retains the upstream build recipe. The project Windows toolchain places Clang
intrinsic headers before Microsoft declarations and selects llvm-ar for GNU
archive rules.

Remove these adaptations when the pinned upstream port and toolchain provide the
same Linux-to-Windows support. Validate both configurations and actual Windows
execution when upgrading.
