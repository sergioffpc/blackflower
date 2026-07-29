# blackflower-script

Safe Rust ownership over a statically linked
[Luau 0.731](https://github.com/luau-lang/luau/releases/tag/0.731) compiler and
virtual machine. The upstream source is pinned as the Git submodule
`vendor/luau` at commit `f8ca77acdcb50241e3da21af663f8ef97b4b5ce4`.

The build enables Luau's official C linkage, compiles `Luau.Compiler` and
`Luau.VM`, and generates private Rust declarations from the public VM headers
and `native/wrapper.h`. All `unsafe` calls remain in `src/ffi.rs`.

## Checkout and prerequisites

Clone the repository and its submodules together:

```sh
git clone --recurse-submodules https://github.com/sergioffpc/blackflower.git
```

For an existing checkout:

```sh
git submodule update --init --recursive
```

The build needs CMake 3.20 or newer, a C++17 compiler, and the libclang shared
library used by bindgen. When libclang is outside the platform's normal search
path, point `LIBCLANG_PATH` at the directory containing `libclang.so`,
`libclang.dylib`, or `libclang.dll`.

To deliberately update Luau, fetch and check out a reviewed release in the
submodule, update the version constants in `native/CMakeLists.txt`, and commit
the new submodule pointer:

```sh
git -C crates/blackflower-script/vendor/luau fetch --tags origin
git -C crates/blackflower-script/vendor/luau checkout 0.731
git add crates/blackflower-script/vendor/luau
```

## Runtime API

The initial safe surface compiles source, loads bytecode, executes chunks, and
copies primitive results out of the VM:

```rust
use blackflower_script::{Runtime, Value};

# fn example() -> Result<(), blackflower_script::Error> {
let mut runtime = Runtime::with_seed(7)?;
let values = runtime.execute(
    "policy.luau",
    "local roll = math.random(1, 6); return roll >= 4, vector.create(1, 2, 3)",
)?;

assert!(matches!(values.first(), Some(Value::Boolean(_))));
assert!(matches!(values.get(1), Some(Value::Vector(_))));
# Ok(())
# }
```

Runtime initialization excludes `os` and `debug`; no filesystem, network, or
module loader is registered. Builtin libraries are frozen through
`luaL_sandbox`, each runtime receives a writable sandbox global table, and
`math.random` is seeded explicitly.

Luau bytecode is not a stable interchange format. Cooked bytecode must carry
the exact Luau/content compatibility identity and be rejected by consumers
using another VM version.

This binding does not yet enforce memory or execution budgets and must not run
untrusted scripts. Tables, functions, userdata, buffers, and host callbacks are
also intentionally outside the initial safe value surface.
