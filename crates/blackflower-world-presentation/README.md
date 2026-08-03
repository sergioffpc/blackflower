# blackflower-world-presentation

`blackflower-world-presentation` turns captured client state into client-only scene
state and immutable backend commands once per displayed frame.

Every variable-step frame uses a caller-supplied `TickDelta` and advances the
ordered `PresentationPipeline` through these phases:

1. `PrepareFrame` prepares bounded timing policy, the frame index, and transient state;
2. `CaptureFrameInputs` captures one immutable `ClientView`, its authoritative
   interpolation window, client events, settings, and backend feedback;
3. `UpdateSceneProxies` synchronizes client-only proxies with simulation
   entities;
4. `SampleRenderTimeline` samples local prediction and remote interpolation;
5. `PrepareViewsAndListeners` prepares view selection, layouts, rigs, and projections;
6. `EvaluateAnimationPoses` evaluates animation, inverse kinematics, and
   procedural poses;
7. `ResolveSceneGraph` resolves hierarchies, attachments, world transforms, and
   final camera/listener poses;
8. `UpdateEffectsAndFeedback` deduplicates cues and advances visual effects,
   audio, UI, and haptics;
9. `BuildFrameOutputs` builds a complete immutable `RenderFrame` and frame-keyed
   audio, UI, and haptic outputs;
10. `PublishFrameOutputs` publishes those outputs to bounded, idempotent backend
    handoffs;
11. `CommitFrameHistory` commits previous-frame state and retires consumed
    inputs and events.

`PresentationWorld` commits the next `FrameIndex` only after every phase
succeeds. On failure, it restores the previous execution context and leaves the
current frame unchanged. Captured simulation and prediction states remain
read-only throughout presentation.

The renderer consumes `RenderFrame` through a single-slot latest-wins mailbox
and owns GPU culling, LOD, uploads, residency, retirement, swapchain, and
presentation. The crate owns no wall-clock pacing, device input collection,
prediction, reconciliation, transport, or concrete audio, UI, or haptic backend.
