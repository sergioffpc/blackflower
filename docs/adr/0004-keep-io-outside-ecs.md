# Keep I/O outside every ECS world

Status: accepted. The owner approved the world phases with the explicit constraint that no I/O may execute inside the ECS.

Systems, observers, hooks, and callbacks executing as part of the Simulation, Prediction, or Presentation World only transform in-memory data. They cannot perform network, file, console/logging, device-input, audio-output, or graphics/presentation I/O, either directly or through a helper. They receive time, input, SDK results, and external status as values and produce requests, snapshots, events, and diagnostics as values.

Application-owned adapters perform effects outside ECS execution. This includes GameNetworkingSockets, human input and agent invocation, asset loading, Steam Audio/device output, Falcor/Slang GPU work, and PhysX SDK operations. Keeping CPU and GPU PhysX behind the same request/result integration avoids an implicit GPU-I/O exception. Adapters never mutate a progressing world or retain pointers into its storage; the owner imports completed data at an explicit safe point.

The phase names describe logical work. `ExportCommands`, `ComposeFeedback`, and `ExportPresentation` only transform or export in-memory data; external adapters perform transmission, playback, and rendering. Physics and reconciliation may require prepare/result stages with an external SDK operation between them. The application drives those stages in order without executing I/O inside a Flecs system or recursively progressing the world. Exact pipeline segmentation remains an implementation detail to validate.

This tightens the previous phase descriptions that placed rendering and SDK completion inside a phase callback. It preserves their logical ordering, world ownership, GPU prediction requirement, and update rates. The cost is explicit buffers and orchestration; the benefit is a world interface that can be exercised with supplied input/results and inspected output, independently of external devices. Integration tests still use the real adapters to establish actual physics and perceptual behavior.

The owner also requires every phase to use the same abstraction level: a world data transformation with explicit memory inputs and outputs. The [phase document](../flecs-phases.md) refines device-oriented names into `ExportCommands`, `ComposeFeedback`, and `ExportPresentation`, preserving the accepted functional order. SDK operations and execution segments are implementation details below this phase vocabulary.
