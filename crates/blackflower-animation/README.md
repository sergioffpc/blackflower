# blackflower-animation

Safe Rust ownership over a statically linked
[ozz-animation 0.16.0](https://github.com/guillaumeblanc/ozz-animation/releases/tag/0.16.0).
The C++ source is pinned as the Git submodule `vendor/ozz-animation` at commit
`6cbdc790123aa4731d82e255df187b3a8a808256`.

Like `blackflower-ecs` and `blackflower-physics`, this crate keeps generated
declarations and every `unsafe` operation in a private `ffi` module. A small C
ABI implemented by `native/wrapper.cpp` isolates Rust from ozz's C++ ABI,
SIMD layouts, archive types and ownership rules.

The runtime surface loads optimized skeleton and animation `.ozz` archives,
reuses ozz sampling contexts, evaluates local poses, blends normal, additive
and per-joint weighted layers, applies aim and two-bone IK, and exposes local
joint transforms plus model-space matrices. Skeletons and clips are immutable
and can be shared between threads. Each character owns mutable sampling
contexts and poses.

Host-driven animation graphs and marker tracks are implemented in Rust. The
graph advances state timing and explicit crossfades; gameplay or policy code
still decides which registered transition to request. Marker tracks report
deterministically ordered timeline crossings, including wrapped playback.

Offline importers and the `*2ozz` conversion tools are deliberately not linked
into the game runtime. Produce trusted runtime archives in the content
pipeline with the matching ozz 0.16 toolchain. The upstream archive reader
assumes well-formed content and is not a sandbox for files supplied by
untrusted users.

## Checkout and prerequisites

Clone the repository and its submodules together:

```sh
git clone --recurse-submodules https://github.com/sergioffpc/blackflower.git
```

For an existing checkout:

```sh
git submodule update --init --recursive
```

The build needs CMake 3.24 or newer, a C++17 compiler and the libclang shared
library used by bindgen. When libclang is outside the platform's normal search
path, point `LIBCLANG_PATH` at the directory containing `libclang.so`,
`libclang.dylib` or `libclang.dll`.

To deliberately update ozz-animation, fetch and check out a reviewed release
in the submodule, then commit the new submodule pointer:

```sh
git -C crates/blackflower-animation/vendor/ozz-animation fetch --tags origin
git -C crates/blackflower-animation/vendor/ozz-animation checkout 0.16.0
git add crates/blackflower-animation/vendor/ozz-animation
```

## Safe API

```rust,no_run
use blackflower_animation::{
    Animation, Pose, SamplingContext, SamplingRatio, Skeleton,
};

# fn example() -> Result<(), Box<dyn std::error::Error>> {
let skeleton_bytes = std::fs::read("assets/character_skeleton.ozz")?;
let animation_bytes = std::fs::read("assets/character_idle.ozz")?;
let skeleton = Skeleton::from_bytes(&skeleton_bytes)?;
let animation = Animation::from_bytes(&animation_bytes)?;

let mut context = SamplingContext::new(animation.track_count())?;
let mut pose = Pose::new(&skeleton)?;
pose.sample(
    &skeleton,
    &animation,
    &mut context,
    SamplingRatio::new(0.5)?,
)?;

for model_matrix in pose.model_matrices() {
    // Upload the joint matrix to the renderer's skinning palette.
    let _ = model_matrix;
}
# Ok(())
# }
```

Blending accepts distinct input and output poses:

```rust,no_run
use blackflower_animation::{BlendLayer, Error, Pose, Skeleton};

# fn blend(skeleton: &Skeleton, first: &Pose, second: &Pose) -> Result<(), Error> {
let mut output = Pose::new(skeleton)?;
let layers = [
    BlendLayer::normal(first, 0.25)?,
    BlendLayer::normal(second, 0.75)?,
];
output.blend(skeleton, &layers, 0.1)?;
# Ok(())
# }
```

Procedural systems can edit validated local transforms with
`Pose::set_local_transform` or `Pose::set_local_transforms`. Aim and two-bone
jobs use model-space targets and update both the local pose and cached
model-space matrices without exposing Ozz SIMD layouts.

`AnimationGraph` deliberately contains no gameplay conditions and owns no
clips. Its evaluation returns state identifiers, normalized sampling ratios and
blend weights that the presentation layer maps to immutable animation assets.
