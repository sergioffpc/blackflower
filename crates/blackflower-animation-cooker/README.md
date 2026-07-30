# Blackflower animation cooker

Host-only glTF to ozz integration for the Blackflower asset pipeline. The crate
builds the pinned `gltf2ozz` executable, invokes it in isolated temporary
directories, inspects its private archives through the safe animation bridge,
and emits `.bfskel` and `.bfanim` containers.

It is not a runtime dependency.

`cook_skeleton` selects one exact named skin and emits one `.bfskel`.
`cook_animation` selects one exact named animation, reads
`animation.extras.blackflower`, validates it against the generated Ozz clip,
and emits one independent `.bfanim`. The supplied `.bfskel` determines the
required rig identity. When the animation source also contains a unique named
skin, its independently cooked identity must match that dependency; an
animation-only source without a skin is mapped against the dependency directly.
All raw Ozz files and exact converter configurations live in a temporary
directory and are removed when the call finishes.

The host tool uses Ozz 0.16.0 at the pinned submodule revision. Sampling,
iframe, optimization, and root-motion tolerances come only from the selected
Blackflower cooking profile.
