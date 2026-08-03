# blackflower-rendering

Backend-facing rendering contracts for the future dedicated renderer thread.

`blackflower-world-presentation` owns scene proxies, animation, hierarchy, final
camera transforms, and construction of a complete immutable `RenderFrame`. It
publishes that snapshot through `LatestFrameMailbox`, a single-slot latest-wins
handoff with monotonic frame IDs. Re-publishing an already consumed or pending
frame ID is idempotently ignored.

The renderer owns resource upload and `Requested -> Uploading -> Resident` or
`Failed` state, GPU culling and LOD, render-graph compilation, command encoding,
swapchain presentation, fallbacks, and GPU-safe retirement. A `RenderFrame`
contains persistent logical handles rather than ECS references or asset bytes.
