# Blackflower native vendor build

This host tool is the only producer for native libraries stored in the
repository-level `vendor/` directory. Build every global vendor for the active
host target and Cargo profile with:

```sh
cargo native build --profile debug
cargo native build --profile release
```

Pass `embree` or `zlib` after the options to build only one vendor. `--target`
accepts the native Rust target triple, and `--crt-static` selects the matching
MSVC CRT contract. Cross-compiling these CMake projects is deliberately not
implicit: CI uses a native runner for every release target.

Artifacts live under
`target/native/<target>/<cmake-profile>/<crt>/<vendor>/`. Each successful build
writes a versioned manifest there. Consumer `build.rs` scripts validate that
manifest and link the exact archive paths; they never configure the global
vendor sources themselves. Set `BLACKFLOWER_NATIVE_DIR` to use a different
shared root. If it is unset, the tool honors `CARGO_TARGET_DIR`.
