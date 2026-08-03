# blackflower-rendering-particles

Presentation-only GPU particle simulation and rendering boundary. The future
runtime will consume cooked definitions and cues from `blackflower-vfx` while
sharing the renderer-owned `wgpu` device, queue, depth resources, render graph,
frame ordering, and GPU-safe retirement.

This crate will own discrete visual particles such as sparks, embers, dust,
tracers, precipitation, and non-authoritative debris. NVIDIA Flow remains the
owner of volumetric fire and smoke; Jolt and Blast remain the owners of
gameplay-relevant rigid bodies and structural fracture. Decals, transient
lights, and post-processing remain renderer concerns rather than particle
state.

Particle state must never drive damage, ignition, AI visibility, collision
authority, or network replication. Replicated domain events and seeds may
produce local presentation, but individual particles remain local GPU state.

The crate is deliberately a scaffold. It contains no particle layout, shader,
allocator, simulation pass, render pass, public API, or resource budget yet.
