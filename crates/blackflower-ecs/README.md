# blackflower-ecs

Safe Rust ownership and query projection over a statically linked
[Flecs 4.1.6](https://github.com/SanderMertens/flecs/releases/tag/v4.1.6).
The C source is pinned as the Git submodule `vendor/flecs` at commit
`fb55f3c25660425cfe1bc4cf5e6bff8b3f18a9b8`.

The native build enables Flecs core, systems, pipelines and the query DSL. The
DSL activates its parser dependency, and pipelines activate modules, systems
and the OS API implementation. C++, REST, HTTP, JSON, scripts, Flecs Metrics
and the other default addons are not compiled. The optional Rust `metrics`
feature enables Flecs Stats and its timer dependency.

## Checkout and prerequisites

Clone the repository and its submodules together:

```sh
git clone --recurse-submodules https://github.com/sergioffpc/blackflower.git
```

For an existing checkout:

```sh
git submodule update --init --recursive
```

The build needs a C compiler and the libclang shared library used by bindgen.
When libclang is installed outside the platform's normal search path, point
`LIBCLANG_PATH` at the directory containing `libclang.so`, `libclang.dylib` or
`libclang.dll`.

To deliberately refresh the vendored source, fetch in the submodule, check out
the reviewed commit, then commit the new submodule pointer in this repository:

```sh
git -C crates/blackflower-ecs/vendor/flecs fetch --tags origin
git -C crates/blackflower-ecs/vendor/flecs checkout fb55f3c25660425cfe1bc4cf5e6bff8b3f18a9b8
git add crates/blackflower-ecs/vendor/flecs
```

## Safe API

Components are plain data. `bytemuck::Pod` excludes references, padding and drop
glue at the Rust boundary:

`Component` and `Tag` are reexported by `blackflower-ecs`. Their procedural
macro implementation lives in the private `derive/` package required by Rust;
workspace consumers must not depend on that package directly.

```rust
use blackflower_ecs::{
    BuiltinPhase, Component, Read, SystemResult, Tag, TickDelta, World, Write,
};
use bytemuck::{Pod, Zeroable};

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(C)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(C)]
struct Velocity {
    x: f32,
    y: f32,
}

#[derive(Tag)]
struct Active;

# fn example() -> Result<(), Box<dyn std::error::Error>> {
let mut world = World::new()?;
let position = world.register_component::<Position>()?;
let velocity = world.register_component::<Velocity>()?;
let active = world.register_tag::<Active>()?;
let entity = world.spawn()?;
world.insert(entity, position, Position { x: 0.0, y: 0.0 })?;
world.insert(entity, velocity, Velocity { x: 1.0, y: 0.0 })?;
world.add_tag(entity, active)?;

let phase = world.builtin_phase(BuiltinPhase::OnUpdate);
world
    .system("Integrate", "Position, [in] Velocity")?
    .phase(phase)?
    .project((Write::<Position>::field(0), Read::<Velocity>::field(1)))?
    .each(|context, _entity, (position, velocity)| -> SystemResult {
        position.x += velocity.x * context.delta().as_seconds();
        Ok(())
    })?;

world.progress(TickDelta::from_seconds(1.0 / 60.0)?)?;
# Ok(())
# }
```

The derives use the Rust type identifier as the stable Flecs name. Override it
when compatibility requires another name:

```rust
# use blackflower_ecs::Tag;
#[derive(Tag)]
#[ecs(name = "Player")]
struct Controllable;
```

Query field indexes are zero-based indexes into the Flecs DSL expression.
`Read<T>`, `Write<T>`, `Optional<F>`, `PairRead<T>` and `PairWrite<T>` perform
runtime checks before references are materialized. Terms can remain
unprojected, so predicates, tags and other DSL-only constraints do not need a
Rust field marker.

`World` is neither `Send` nor `Sync`. `parallel_each` gives Flecs a
`Send + Sync` callback but exposes only the projected data for the current
entity. Structural changes are available as deferred `Commands` only in
single-threaded `each` systems.

System errors and panics are captured at the C trampoline. Later Rust callbacks
in that tick become no-ops and `progress` or `run_pipeline` returns the first
`RunError`. Work completed before the failure is not rolled back.

## Optional observability

Observability is disabled by default and does not change the safe public API:

```toml
blackflower-ecs = { path = "../blackflower-ecs", features = ["observability"] }
```

The feature flags can also be enabled independently:

- `tracing` emits lifecycle events, registration events, callback failures and
  a trace-level span around `progress` and `run_pipeline`;
- `metrics` emits Rust `metrics` facade signals and compiles Flecs with
  `FLECS_STATS`. It does not enable the separate `FLECS_METRICS` addon;
- `observability` enables both.

This library never installs a global `tracing` subscriber or `metrics`
recorder. Applications remain responsible for choosing and configuring their
subscriber, recorder and exporter. If none is installed, the instrumentation
has no external output. Install them before constructing a `World` so lifecycle
events and the initial aggregate gauges are observed.

The metrics use the `blackflower_ecs_` prefix. They cover active worlds,
registered resources, tick results and duration, callback failures, aggregate
entity/table/query/system gauges and per-tick Flecs distributions for systems,
internal frame/system/merge/rematch timing, merges, rematches, pipeline rebuilds
and deferred commands. Labels are limited to fixed operation, result,
resource-kind and failure-kind values; entity IDs, component names and system
names are never metric labels.

Wall-clock timings are diagnostic only. They must not be fed back into
simulation decisions, authoritative state or replay. Runtime statistics are
reported after each tick, including ticks that return a callback failure,
because earlier work is not rolled back.
