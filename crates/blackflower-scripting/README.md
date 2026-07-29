# blackflower-scripting

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
git -C crates/blackflower-scripting/vendor/luau fetch --tags origin
git -C crates/blackflower-scripting/vendor/luau checkout 0.731
git add crates/blackflower-scripting/vendor/luau
```

## Runtime API

The initial safe surface compiles source, loads bytecode, executes chunks, and
copies primitive results out of the VM:

```rust
use blackflower_scripting::{
    Library, Runtime, RuntimeConfig, SandboxPolicy, Value,
};

# fn example() -> Result<(), blackflower_scripting::Error> {
let libraries = SandboxPolicy::standard()
    .with_library(Library::Coroutine, false)
    .with_library(Library::Buffer, false);
let config = RuntimeConfig::default()
    .with_random_seed(7)
    .with_vm_memory_limit_bytes(8 * 1024 * 1024)
    .with_execution_fuel(50_000)
    .with_sandbox_policy(libraries);
let mut runtime = Runtime::with_config(config)?;
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

`RuntimeConfig::default()` limits each VM to 16 MiB and restores 100,000 fuel
units before every execution. Fuel counts interruptible VM safepoints such as
loop back-edges and calls rather than individual bytecode instructions. The
allocator rejects growth above the configured ceiling; `Runtime::memory_usage`
reports current, peak, and limit values. Exhaustion is reported as
`Error::ExecutionLimit` or `Error::OutOfMemory`, and the runtime remains usable
for subsequent chunks.

The library policy is an allowlist. It can remove any of the safe standard
libraries supported by the crate, but it can never enable `os`, `debug`,
filesystem, networking, or module loading.

Luau bytecode is not a stable interchange format. Cooked bytecode must carry
the exact Luau/content compatibility identity and be rejected by consumers
using another VM version.

The VM memory ceiling does not cover the standalone C++ compiler used by
`compile`. Cook untrusted source in a separately constrained worker and run
only size-capped, identity-checked bytecode in the authoritative simulator.
Tables, functions, userdata, buffers, and host callbacks remain intentionally
outside the initial safe result surface.
