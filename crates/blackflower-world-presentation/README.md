# blackflower-world-presentation

`blackflower-world-presentation` turns captured client state into client-only scene
state and immutable backend commands once per displayed frame.

Every variable-step frame uses a caller-supplied `TickDelta` and advances the
ordered `PresentationPipeline` through these phases:

1. `PrepareFrame` prepares the frame index, timing context, and transient state;
2. `CaptureFrameInputs` captures immutable prediction state, snapshots, events,
   and settings;
3. `UpdateSceneProxies` synchronizes client-only proxies with simulation
   entities;
4. `SampleRenderTimeline` samples local prediction and remote interpolation;
5. `UpdateCamerasAndListeners` updates views and spatial-audio listeners;
6. `EvaluateAnimationPoses` evaluates animation, inverse kinematics, and
   procedural poses;
7. `ResolveSceneGraph` resolves hierarchies, attachments, and world transforms;
8. `UpdateEffectsAndFeedback` advances visual effects, audio, UI, and haptics;
9. `BuildBackendCommands` builds immutable commands for active client backends;
10. `SubmitBackendCommands` submits those commands to the backends;
11. `CommitFrameHistory` commits previous-frame state and retires consumed
    inputs and events.

`PresentationWorld` commits the next `FrameIndex` only after every phase
succeeds. On failure, it restores the previous execution context and leaves the
current frame unchanged. Captured simulation and prediction states remain
read-only throughout presentation.

The crate owns no wall-clock pacing, device input collection, prediction,
reconciliation, transport, or concrete rendering, audio, UI, or haptic backend.
