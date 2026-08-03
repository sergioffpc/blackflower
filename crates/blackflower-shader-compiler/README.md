# blackflower-shader-compiler

`blackflower-shader-compiler` is Blackflower's private native compiler boundary
for offline shader cooking. It builds the pinned Slang 2026.14.1 source at
commit `7c58a326b1f3812411a204b19cb01e323d8f6010` as static libraries and exposes
a small safe Rust API that compiles one Slang entry point to SPIR-V 1.5. The
caller supplies a virtual source name for debug metadata; the cooker uses the
portable logical asset path instead of a host filesystem path.

The crate does not choose a graphics backend. The asset cooker owns target,
optimization, and debug policy through a selected cooking profile, validates
the generated module with Naga, and stores the SPIR-V bytes. Runtime loading is
outside this compiler; it can later pass those bytes to wgpu for translation to
Metal, Vulkan, or Direct3D 12.

Source imports and includes are deliberately unsupported by this compiler, so one
asset and its content hash fully describe one shader compilation input.

## Setup

Initialize Slang and its pinned nested dependencies:

```sh
git submodule update --init --recursive \
    vendor/slang
```

Building requires CMake, a C++17 compiler, and libclang for build-time bindgen.
If libclang is not discoverable, set `LIBCLANG_PATH` to the directory containing
its shared library.

Run the focused checks:

```sh
cargo clippy --package blackflower-shader-compiler --all-targets -- -D warnings
cargo test --package blackflower-shader-compiler
```
