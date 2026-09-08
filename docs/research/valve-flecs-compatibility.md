# Flecs and Valve stack compatibility

Research date: 2026-09-07. Scope: the owner's selected Flecs,
GameNetworkingSockets, and Steam Audio stack for the LAN MVP. This is source
inspection, not a successful build or runtime test.

## Existing build constraint

Blackflower targets Windows from Linux using the GNU-style Clang 21 driver with
the `x86_64-pc-windows-msvc` target, LLD, and xwin-provided Microsoft
headers/libraries. The custom triplet uses static third-party libraries and the
dynamic Microsoft CRT. Project-owned C++ currently disables exceptions. See the
[build guide](../build.md) and [C++ policy](../cpp-guidelines.md).

## Flecs

Flecs provides an ECS with C and C++ interfaces. Its upstream CMake project
builds the core as C, offers static/shared targets, and links Windows system
libraries or Linux pthreads according to the target. The public build
documentation covers integration and feature selection. These facts make it a
reasonable cross-build candidate; they do not validate Blackflower's exact
compiler/CRT combination. Sources:
[upstream CMake](https://github.com/SanderMertens/flecs/blob/master/CMakeLists.txt),
[building Flecs](https://www.flecs.dev/flecs/md_docs_2BuildingFlecs.html).

The main upstream license is MIT, with notices to retain for bundled third-party
code. The selected vcpkg port explicitly lists additional notices. Sources:
[Flecs license](https://github.com/SanderMertens/flecs/blob/master/LICENSE),
[pinned Flecs port](https://github.com/microsoft/vcpkg/blob/9e593bb18ea69cc5095e012465dcd675a822ed0d/ports/flecs/portfile.cmake).

Updated owner decision: use one authoritative Simulation World on the server and
two separate worlds in each client, Prediction World and Presentation World, fed
through common human/agent input. See
[ADR-0002](../adr/0002-separate-client-worlds-and-input.md). Use project-owned
player identifiers on the wire; do not serialize process-local Flecs identifiers
or raw component memory. Keep ECS access inside the modules that own each world.
Test the chosen C++ interface under the existing exception policy; absence of a
documented exception requirement is not a compile result.

## GameNetworkingSockets

The likely intended name for the owner's “gameservicesocket” is Valve
**GameNetworkingSockets**. It provides connection-oriented, message-based
transport over UDP with reliable and unreliable delivery and measurement
facilities. Steam installation/accounts are not required for its open-source
direct-IP operation; some Steam-specific services belong to Steamworks. Entity
serialization and state replication remain application responsibilities. Source:
[Valve's repository documentation](https://github.com/ValveSoftware/GameNetworkingSockets).

Its build needs a crypto provider and Protobuf; upstream documents Linux and
Windows builds. The checked CMake source has distinct Windows/Clang handling,
but no exact Blackflower cross-build proof was found. Sources:
[upstream build instructions](https://github.com/ValveSoftware/GameNetworkingSockets/blob/master/BUILDING.md),
[CMake configuration](https://github.com/ValveSoftware/GameNetworkingSockets/blob/master/CMakeLists.txt).

Proposal: use direct-IP connections over the LAN, disable the optional ICE/P2P
feature, and retain OpenSSL as selected by the vcpkg port. Use reliable messages
for admission and discrete events, and sequence-numbered replaceable messages
for ongoing input/state. The protocol must define ordering, limits, and identity
independently of the transport.

## Steam Audio and device output

Steam Audio exposes a C interface and publishes Windows and Linux libraries. Its
integration guide expects an audio engine/mixer to handle output and explains
how to pass game-thread results into audio processing. Therefore selecting Steam
Audio does not by itself select an audio device/playback backend. Sources:
[C API setup](https://valvesoftware.github.io/steam-audio/doc/capi/getting-started.html),
[audio engine integration](https://valvesoftware.github.io/steam-audio/doc/capi/integration.html).

The current vcpkg port uses host FlatBuffers tools separately from target
libraries, and disables optional IPP, Embree, Radeon Rays, and TrueAudio Next
dependencies. This reduces the initial integration surface. Verify the actual
Windows import/static libraries, CRT, and callback boundaries in the
cross-build. Source:
[pinned Steam Audio port](https://github.com/microsoft/vcpkg/blob/9e593bb18ea69cc5095e012465dcd675a822ed0d/ports/steam-audio/portfile.cmake).

Proposal: add miniaudio's device layer, using WASAPI on Windows, behind the
audio module. Its low-level interface provides playback/capture callbacks and
WASAPI loopback support. This is an additional supporting dependency proposal,
not an owner-selected requirement. Source:
[miniaudio manual](https://miniaud.io/docs/manual/index.html).

For the agreed MVP, bypass spatial effects and preserve the constant,
non-spatial hit signal. Steam Audio remains the selected acoustic processing
integration; selecting it does not add HRTF, distance attenuation, reflections,
or propagation to the MVP scope. Isolating output per client remains necessary
for the four-process audio acceptance check.

## Candidate versions from the existing registry pin

The following were read from the exact `builtin-baseline` commit already in the
manifest, using the local registry checkout. They are available candidates, not
installed or accepted integration versions.

| Port                  | Baseline version | Observation                                                                            |
| --------------------- | ---------------- | -------------------------------------------------------------------------------------- |
| flecs                 | 4.1.6            | No gameplay dependency has been added to Blackflower yet.                              |
| gamenetworkingsockets | 1.6.0            | ICE is a default feature at this pin; explicitly disable it for the LAN-only proposal. |
| steam-audio           | 4.8.1            | Port declares Apache-2.0; preserve its third-party notices.                            |
| miniaudio             | 0.11.25          | Optional supporting device backend proposal.                                           |

Source:
[pinned registry baseline](https://github.com/microsoft/vcpkg/blob/9e593bb18ea69cc5095e012465dcd675a822ed0d/versions/baseline.json),
[GNS manifest](https://github.com/microsoft/vcpkg/blob/9e593bb18ea69cc5095e012465dcd675a822ed0d/ports/gamenetworkingsockets/vcpkg.json),
[Steam Audio manifest](https://github.com/microsoft/vcpkg/blob/9e593bb18ea69cc5095e012465dcd675a822ed0d/ports/steam-audio/vcpkg.json).

## Verification still required

Compile/link the selected libraries with the repository's exact target flags,
exercise no-throw project call boundaries, run a direct-IP Linux/Windows
exchange, and emit/capture a signal on Windows. Validate the Linux runtime ABI
against the actual Debian server. Neither platform support in documentation nor
a vcpkg port establishes these results.
