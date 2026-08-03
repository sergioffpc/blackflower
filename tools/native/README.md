# Blackflower native vendor build

This host tool is the only producer for native libraries stored in the
repository-level `vendor/` directory. Build every global vendor for the active
host target and Cargo profile with:

```sh
cargo native build --profile debug
cargo native build --profile release
```

Pass one or more vendor names after the options to build a focused subset. The
tool resolves native dependencies automatically; for example, `steam-audio`
also prepares Embree, FlatBuffers, libmysofa, PFFFT, and zlib. Run
`cargo native build --help` for the complete vendor list. `--target` accepts the
native Rust target triple, and `--crt-static` selects the matching MSVC CRT
contract. Cross-compiling these CMake projects is deliberately not implicit:
CI uses a native runner for every release target.

Artifacts live under
`target/native/<target>/<cmake-profile>/<crt>/<vendor>/`. Each successful build
writes a versioned manifest there. Consumer `build.rs` scripts validate that
manifest and link the exact archive paths; they never configure the global
vendor sources themselves. Set `BLACKFLOWER_NATIVE_DIR` to use a different
shared root. If it is unset, the tool honors `CARGO_TARGET_DIR`.

## Layout

- `src/main.rs` owns the command-line interface.
- `src/vendor.rs` owns vendor selection, dependencies, and dispatch.
- `src/vendor/<vendor>.rs` owns the build recipe for one vendor.
- `src/vendor/common.rs` contains helpers shared by the build recipes.
- `../../crates/blackflower-build` defines the artifact manifest, lookup, and
  linking contract shared with the FFI crates' build scripts.
