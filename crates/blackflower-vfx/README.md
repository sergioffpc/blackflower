# blackflower-vfx

Backend-neutral contracts for authored visual effects, cooked effect assets,
and presentation cues. This crate will define what an effect means without
selecting how particles, fluids, decals, lights, or post-processing are
implemented.

Authoritative simulation emits domain events. Presentation translates those
events into bounded, deduplicated visual-effect cues and owns their lifetime.
This crate must not become an authority for damage, ignition, AI visibility,
physics, networking, or gameplay state.

The crate is deliberately a scaffold. It defines no public API, asset
extension, schema version, or serialization format yet.
