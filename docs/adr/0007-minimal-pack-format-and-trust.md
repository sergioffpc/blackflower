# Define the minimal pack format and independent content trust

The pack-role and format decisions below are superseded by
[ADR-0010](0010-agnostic-content-packs.md). Its
[entity amendment](0010-agnostic-content-packs.md#entity-contract-amendment)
also supersedes the single-file source restriction and integer-millimetre
encoding below.

Status: accepted implementation decision for
[#21](https://github.com/sergioffpc/blackflower/issues/21). The owner
additionally requires OpenUSD scenario sources, uv for Python, and separate
client/server filename extensions. These requirements supplement ADR-0005
without changing the selected model-import or runtime SDK roles.

The original size caps are superseded by
[ADR-0009](0009-typed-errors-and-content-size-policy.md); independent trust and
cryptographic choices remain applicable.

Use the uncompressed [pack v1](../../schemas/pack/v1.md), with explicit
little-endian fields, integer-millimetre
[scene values](../../schemas/scene/v1.md), SHA-256 resource digests and detached
Ed25519 signatures over exact header and manifest bytes. Each artifact has its
own PackId; an authenticated ScenarioBuildId binds both roles' source, settings,
provenance and resources without including signatures or final identities in its
own derivation. The extensions are `.bfclient` and `.bfserver`; authentication
checks the embedded role regardless of the filename.

Use cryptography 46.0.5 in Python and libsodium 1.0.22#1 in C++. The standard
library and Boost do not provide the required Ed25519/SHA-256 combination.
OpenSSL EVP was considered, but its vcpkg Windows port assumes Windows-host
build tools. Libsodium offers a small C interface without C++ exceptions and an
existing Clang cross-build route. A narrowly scoped overlay corrects its
Linux-host dependency and Autotools linker/CRT setup. The distinct Python and
C++ crypto implementations also strengthen conformance testing. Dependencies
remain pinned by uv.lock and the vcpkg manifest/overlay.

Trust is an application-owned public-key set supplied independently of content.
The loader never trusts keys supplied by the pack. The test harness explicitly
accepts a raw public-key file; future product applications can compile their
trust set into the executable. Private content-signing keys are separate from
Git signing and never committed or deployed. Disposable fixture keys exercise
the same verification path without a bypass.

The cooker reads a self-contained OpenUSD primitive scene using official USD
bindings. External composition and richer assets require a future dependency and
provenance contract. This initial restriction avoids silently omitting
referenced inputs from the build identity. Exact source bytes are hashed and
parsed from a snapshot; USD is not linked into the runtime. Integer millimetres
make primitive equality portable, at the cost of an explicit 0.001 mm source
conversion tolerance and a future schema revision for finer geometry.

Publish two verified completed files by renaming a sibling staging directory
into a new output directory. An exclusive sibling reservation coordinates
concurrent cooker processes; a process killed abruptly may leave a reservation
for an operator to inspect. This local-filesystem publication contract avoids
pretending that two independent file renames are one atomic pair update.
