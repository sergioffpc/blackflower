# Isolate Falcor exception handling at the graphics adapter

Status: proposed; requires the owner's decision before changing project compiler policy.

The owner selected Falcor for rendering while project-owned C++ currently disables exceptions. Falcor's public error machinery includes throwing and exception-handling facilities. Exposing those headers and operations across the client would make the current policy difficult to preserve. Source: [Falcor 8.0 error interface](https://github.com/NVIDIAGameWorks/Falcor/blob/8.0/Source/Falcor/Core/Error.h).

Propose allowing exception handling only in a designated Falcor-facing graphics adapter target. It catches framework errors and returns explicit project-owned error results at a small interface. The simulation, networking, client application, and tests outside that adapter retain the existing no-exceptions policy. No framework type or exception may cross that interface, and no throwing project callback may unwind into a no-exceptions caller.

This is a deliberate change to [the C++ guideline](../cpp-guidelines.md#project-adaptations), not an already approved compiler setting. Keep the proposal conditional until the real Falcor headers and callback paths have passed a compile/link and deliberate-error test with the chosen toolchain.

## Alternatives and consequences

- Enabling exceptions throughout the project would enlarge the affected error model and invalidate the established convention beyond what Falcor integration needs.
- Keeping every project target exception-free would require a compatible upstream wrapper/patch or another proven containment approach; none has been demonstrated.
- A separate renderer process could contain the framework but would add IPC, deployment, and synchronization work to this small MVP.

The proposed adapter contains the policy change and makes framework replacement more local, at the cost of one explicit build/error interface and tests for cleanup and error translation. The owner-selected stack remains unchanged; its Linux-to-Windows build feasibility is a separate question.
