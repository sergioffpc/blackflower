# blackflower-destruction

`blackflower-destruction` owns the authoritative NVIDIA Blast integration for
destructible chunk and bond graphs. The public API uses Blackflower domain
types; Blast pointers, layouts, allocators, and fracture buffers remain behind
a private C ABI.

The first vertical surface supports:

- immutable assets built from authored chunks and bonds;
- one mutable family per destructible instance;
- direct chunk and bond fracture commands and resulting events;
- deterministic actor enumeration, visible-chunk queries, and actor splitting;
- `NvBlastExtStress` on supported x86-64 native targets.

Blast 5.0.6 is pinned through the shared `vendor/PhysX` submodule. The upstream
stress solver is x86 SIMD-only at this revision, so Apple Silicon and other
non-x86 targets retain Low Level destruction but report stress as unsupported.
The wrapper never runs upstream setup scripts from Cargo.

## Technical debt

### ARM64 ExtStress support

**Status:** Open.

Blast 5.0.6 contains a scalar `CGNR_SISD` stress-solver path, but its shared
solver headers unconditionally include x86 intrinsics, declare `__m128` and
`__m256` specializations, and probe SSE, AVX, and FMA through CPUID. The native
Blackflower build therefore excludes the complete ExtStress source set on
non-x86-64 targets instead of selecting the existing scalar path.

This prevents authoritative stress-driven fracture on ARM64 servers and Apple
Silicon development machines. Low Level assets, direct fracture, actor splitting,
and topology queries remain available.

Resolve this debt by:

1. isolating the x86 SIMD types, specializations, and device probe behind an
   architecture-specific backend boundary;
2. compiling and selecting the scalar solver on ARM64 before considering an
   optional NEON implementation;
3. enabling the ExtStress sources and wrapper surface in the ARM64 native build;
4. adding force, convergence, overstressed-bond, fracture-command, repetition,
   and cross-architecture golden tests; and
5. benchmarking the scalar backend before treating NEON optimization as a
   requirement.

The debt is closed when ARM64 reports stress support and passes the same
authoritative conformance suite as x86-64. Until cross-architecture fracture
outcomes are proven equivalent, deployments with incompatible selected stress
backends must use different `ProtocolRevision` values and cannot share a
session.

Jolt remains responsible for rigid bodies and contacts. This crate reports
chunk/bond topology changes for the simulation layer to materialize in Jolt;
it does not own physics bodies, ballistics, damage policy, rendering, or
replication.
