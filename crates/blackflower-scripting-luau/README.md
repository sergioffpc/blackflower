# blackflower-scripting-luau

Safe Rust ownership over a statically linked
[Luau 0.731](https://github.com/luau-lang/luau/releases/tag/0.731) compiler and
virtual machine. The upstream source is pinned as the Git submodule
`vendor/luau` at commit `f8ca77acdcb50241e3da21af663f8ef97b4b5ce4`.

The build enables Luau's official C linkage, compiles `Luau.Compiler`,
`Luau.CodeGen`, and `Luau.VM`, and generates private Rust declarations from
the public VM headers and `native/wrapper.h`. All `unsafe` calls remain in
`src/ffi.rs`.

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
git -C vendor/luau fetch --tags origin
git -C vendor/luau checkout 0.731
git add vendor/luau
```

## Runtime API

The initial safe surface compiles source, loads bytecode, executes chunks, and
copies primitive results out of the VM:

```rust
use blackflower_scripting_luau::{
    Library, Runtime, RuntimeConfig, SandboxPolicy, Value,
};

# fn example() -> Result<(), blackflower_scripting_luau::Error> {
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

`compile` reports parser and compiler diagnostics immediately rather than
returning Luau's encoded error payload as executable bytecode.

## Compiler profiles

`CompileOptions` carries the three settings selected by the cooking profile
across the safe Rust and native C ABI boundaries:

- `optimization` changes Luau bytecode generation. `Baseline` preserves useful
  debugging behavior; `Aggressive` also permits transformations such as
  inlining.
- `debug` controls bytecode metadata. `LineInfo` provides source locations in
  runtime errors, stack traces, breakpoints, and stepping. `Full` additionally
  retains named locals and upvalues for debugger inspection.
- `type_info` controls which loaded modules are eligible for native codegen.
  `NativeModules` limits compilation to modules marked with `--!native`;
  `AllModules` allows every loaded module.

The `Bytecode` wrapper retains the options used to compile it. Authenticated
cooked content must use `Bytecode::from_bytes_with_options` so the runtime can
apply the matching native-codegen policy. Coverage remains disabled by the
asset cooker.

## Debugging

Debug metadata does not expose Luau's `debug` standard library to scripts.
Instead, the host can run a chunk with breakpoints or single stepping and
receive synchronous snapshots of the call stack, locals, and upvalues:

```rust
use blackflower_scripting_luau::{
    CompileOptions, DebugAction, DebugLevel, DebugOptions, Runtime,
};

# fn example() -> Result<(), blackflower_scripting_luau::Error> {
let mut runtime = Runtime::new()?;
let compile_options = CompileOptions {
    debug: DebugLevel::Full,
    ..CompileOptions::default()
};
let debug_options = DebugOptions::default().with_breakpoint(3);
let mut handler = |event: &blackflower_scripting_luau::DebugEvent| {
    let frame = &event.frames[0];
    assert_eq!(frame.current_line, Some(3));
    DebugAction::Continue
};

let values = runtime.execute_with_options_debugged(
    "policy.luau",
    "local function decide()\n    local accepted = true\n    return accepted\nend\nreturn decide()",
    compile_options,
    &debug_options,
    &mut handler,
)?;
assert_eq!(values.len(), 1);
# Ok(())
# }
```

The handler runs on the runtime's owning thread. It must not re-enter that
runtime. Panics are contained and returned as `Error::DebugHandlerPanicked`.
Luau 0.731 cannot debug native frames, so native execution is temporarily
suspended during `execute_bytecode_debugged` and restored afterward.

## Native codegen

Native codegen is opt-in because it owns executable memory separately from the
VM allocator. Enable it with an explicit budget:

```rust
use blackflower_scripting_luau::{
    CompileOptions, Runtime, RuntimeConfig, TypeInfoLevel,
};

# fn example() -> Result<(), blackflower_scripting_luau::Error> {
let config = RuntimeConfig::default()
    .with_native_codegen_limit_bytes(8 * 1024 * 1024);
let mut runtime = Runtime::with_config(config)?;
let values = runtime.execute_with_options(
    "hot-path.luau",
    "--!native\nlocal function sum(n)\n    local total = 0\n    for i = 1, n do total += i end\n    return total\nend\nreturn sum(100)",
    CompileOptions {
        type_info: TypeInfoLevel::NativeModules,
        ..CompileOptions::default()
    },
)?;

assert_eq!(values.len(), 1);
assert!(runtime.last_native_codegen_stats().is_some());
assert!(runtime.native_codegen_memory_usage().current_bytes > 0);
# Ok(())
# }
```

`native_codegen_supported` reports target support before runtime creation.
`Runtime::last_native_codegen_stats` reports the most recent chunk, while
`Runtime::native_codegen_memory_usage` reports current, peak, and configured
executable-memory usage. A zero budget keeps the interpreter-only default.

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

The asset cooker reads compile options from the selected versioned cooking
profile and emits `luau_bytecode` assets. Runtime composition can reconstruct
the safe owned wrapper with `Bytecode::from_bytes`; the VM validates the
bytecode version and structure when the chunk is loaded.

The VM memory ceiling does not cover the standalone C++ compiler used by
`compile`. Cook untrusted source in a separately constrained worker and run
only size-capped, identity-checked bytecode in the authoritative simulator.
Tables, functions, userdata, buffers, and host callbacks remain intentionally
outside the initial safe result surface.
