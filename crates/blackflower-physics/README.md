# blackflower-physics

Safe Rust ownership over a statically linked
[Jolt Physics 5.6.0](https://github.com/jrouwe/JoltPhysics/releases/tag/v5.6.0).
The C++ source is pinned as the Git submodule `vendor/JoltPhysics` at commit
`e77f175595e64cb44218cc9d9d56fc365ad0e36a`.

Like `blackflower-ecs`, the crate keeps generated declarations and every
`unsafe` operation in a private `ffi` module. A small C ABI implemented by
`native/wrapper.cpp` isolates the public Rust API from Jolt's C++ ABI,
ownership rules and global initialization.

The native build uses C++17, single-precision positions, static linking and
Jolt's `Distribution` configuration. Cross-platform deterministic mode is
enabled. Linux x86_64 builds target AVX2 for Dell PowerEdge R630 servers;
AVX-512 and fused multiply-add remain disabled. macOS ARM64 builds use Jolt's
automatic NEON path. The debug renderer, profiler, object stream and GPU/CPU
compute backends are disabled.

## Checkout and prerequisites

Clone the repository and its submodules together:

```sh
git clone --recurse-submodules https://github.com/sergioffpc/blackflower.git
```

For an existing checkout:

```sh
git submodule update --init --recursive
```

The build needs CMake 3.20 or newer, a C++17 compiler and the libclang shared
library used by bindgen. When libclang is outside the platform's normal search
path, point `LIBCLANG_PATH` at the directory containing `libclang.so`,
`libclang.dylib` or `libclang.dll`.

To deliberately update Jolt, fetch and check out a reviewed release in the
submodule, then commit the new submodule pointer:

```sh
git -C crates/blackflower-physics/vendor/JoltPhysics fetch --tags origin
git -C crates/blackflower-physics/vendor/JoltPhysics checkout v5.6.0
git add crates/blackflower-physics/vendor/JoltPhysics
```

## Safe API

The initial surface supports worlds, sphere and box bodies, body lifetime,
position, linear velocity, broad-phase optimization and fixed simulation
steps. The API uses the SIMD-backed `glam::Vec3A` and `glam::Quat` types
directly; consumers must import them from `glam`:

```rust
use std::num::NonZeroU32;

use blackflower_physics::{
    BodySettings, MotionType, Shape, StepDelta, World,
};
use glam::Vec3A;

# fn example() -> Result<(), blackflower_physics::Error> {
let mut world = World::new()?;
let floor = BodySettings::new(
    Shape::cuboid(Vec3A::new(100.0, 1.0, 100.0))?,
    MotionType::Static,
)
.with_position(Vec3A::new(0.0, -1.0, 0.0))?;
world.create_body(floor)?;

let sphere = BodySettings::new(Shape::sphere(0.5)?, MotionType::Dynamic)
    .with_position(Vec3A::new(0.0, 2.0, 0.0))?;
let sphere = world.create_body(sphere)?;
world.set_linear_velocity(sphere, Vec3A::new(0.0, -5.0, 0.0))?;

world.step(
    StepDelta::from_seconds(1.0 / 60.0)?,
    NonZeroU32::MIN,
)?;
# Ok(())
# }
```

`BodyId` values are tied to their creating world and stale handles are
rejected. `World` is neither `Send` nor `Sync`; Jolt's worker pool remains an
internal implementation detail of `World::step`.
