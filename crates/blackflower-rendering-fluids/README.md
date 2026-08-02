# blackflower-rendering-fluids

`blackflower-rendering-fluids` is the private NVIDIA Flow integration boundary
for presentation-only fire and smoke. It wraps `NvFlowContextOpt` around a
backend-neutral Rust callback interface and provides `WgpuBackend`, which owns
Flow resources on a shared `wgpu::Device`/queue and encodes compute and copy
passes into a per-frame command buffer.

The adapter supports upload and readback buffers, sampled/storage textures,
samplers, SPIR-V compute pipelines, bind groups, dispatch, and all Flow copy
passes. `Context::backend_mut()` exposes `begin_frame`, `finish_commands`, and
`submit` so the presentation renderer controls ordering and completion frames.
The renderer must request `WgpuBackend::required_features()` when creating its
device; Flow's core Grid uses zero-border samplers.

The intended frame integration is:

```text
WgpuBackend::begin_frame(current, completed)
Flow Grid update (future Grid wrapper)
Context::flush()
WgpuBackend::finish_commands()
renderer submits the Flow command buffer in presentation order
```

This crate deliberately does not yet claim that the full Flow Grid is running.
That requires the Flow-specific Slang shader cook and building the Grid/operator
sources that consume its generated headers. The adapter rejects descriptor
arrays, texel buffers, and combined texture/sampler bindings explicitly instead
of silently translating them to an incompatible WebGPU resource model.

Flow 2.2.0 is pinned through the shared `vendor/PhysX` submodule. Only
`NvFlowContextOpt`, its thread-pool support, and required shared code are built;
the upstream Vulkan/DX12 devices, editor, GLFW, and ImGui are excluded.

Flow state is presentation state. This crate must never become authoritative
for damage, AI visibility, ignition decisions, or network replication.
