# Minimal signed scenario pipeline

[#21](https://github.com/sergioffpc/blackflower/issues/21) implements the first
OpenUSD-to-signed-pack-to-C++ content path. This is primitive content and a
runtime consumption harness; graphics, physics execution, audio, admission, and
world startup remain in their dependent tickets.

## Python setup and checks

Use uv 0.10.4 or a reviewed compatible version. The repository pins the Python
minor series in the cooker `.python-version`; the validation baseline is CPython
3.14.4. Runtime dependencies and development tooling are in `uv.lock`. The
setuptools build backend is pinned separately in `pyproject.toml`.

```sh
uv sync --project tools/cooker --locked --no-editable
uv run --project tools/cooker --locked --no-sync mypy \
  --config-file tools/cooker/pyproject.toml tools/cooker/src \
  tests/integration/content_pipeline_test.py
```

After changing package source, reinstall its non-editable build before testing:

```sh
uv sync --project tools/cooker --locked --no-editable \
  --reinstall-package blackflower-cooker
```

The locked dependencies are cryptography 46.0.5 and usd-core 26.8. OpenUSD is a
Linux cooker dependency only. The package imports the official `pxr` bindings,
reads USDA or USDC, and rejects unsupported authoring content explicitly. The
[authoring contract](../schemas/scene/v1.md) and
[reference scene](../assets/scenes/mvp.usda) define units, transforms, IDs,
geometry, light and spawn-point encoding.

## Cooking and verification

Provision a dedicated Ed25519 signing key in PEM/PKCS8 form outside this
checkout and runtime staging. The CLI accepts an unencrypted PEM file; use
filesystem access controls or an external signing adapter for protected keys.
The public `cook_pair` interface accepts a signer callable and raw public key,
so signing authority does not belong to the scene or runtime loader. Future
hardware/key-service integrations can implement that existing external seam.

```sh
uv run --project tools/cooker --locked --no-sync blackflower-cooker cook \
  --source assets/scenes/mvp.usda \
  --output build/packs/mvp \
  --private-key /path/outside/checkout/content-signing.pem
```

The output directory must not exist. Success produces exactly `mvp.bfsimulation`
and `mvp.bfpresentation` and prints their two PackIds and common ScenarioBuildId
as JSON. Each finished file is independently reopened and verified before the
pair is published. Failure exits nonzero with a concrete console error. Existing
output is preserved; use a new directory for a recook. A sibling `.lock` file
coordinates publishers. If a process is killed, confirm it is no longer active
before removing its stale lock or private staging data. No power-loss durability
or network-filesystem transaction guarantee is made.

Run the C++ harness with an independently provisioned raw 32-byte public key:

```sh
build/debug/blackflower_content_harness \
  build/packs/mvp/mvp.bfsimulation simulation /path/to/content-public.key
build/debug/blackflower_content_harness \
  build/packs/mvp/mvp.bfpresentation presentation /path/to/content-public.key
```

The role argument selects simulation or presentation content on any supported
host. The server consumes simulation content; the client uses that same content
for prediction and also consumes presentation content. Callers supply their
required role and independent trust set. The public `VerifiedPack` retains the
read-only file mapping and exposes immutable scene values. Keep backing files
unchanged until all pack copies are released. Hashing touches the whole payload;
decoded scene values are allocated separately. SDK resource creation and pack
I/O remain outside ECS.

The [pack v1 contract](../schemas/pack/v1.md) has no platform profile.

## Validation

The normal native `check` target includes the installed Python package's type
checks and production-to-consumption integration suite. Tests use disposable
signing keys, the real C++ harness, and independently encoded shared fixtures.
Functional coverage checks compatibility, production and independent consumption
of prepared content, and ownership of loaded data. Follow the
[application test scope](development-process.md#current-application-test-scope).

Compile with the [normal checks](build.md#verification). Windows cross-builds
also use `analyze` and must then be executed on Windows. WSL2 can execute the
Windows harness directly; the test driver converts data paths through `wslpath`:

```sh
BLACKFLOWER_CONTENT_HARNESS="$PWD/build/windows-release/blackflower_content_harness.exe" \
  uv run --project tools/cooker --locked --no-sync \
  python tests/integration/content_pipeline_test.py
```

For direct GoogleTest execution, use `tests/fixtures/packs` as the working
directory. CTest prepares its fixture directory automatically. Debug execution
also needs the ASan DLL and the same release CRT as Release, as described in the
build guide. Copy only the appropriate pack, public trust and runtime
dependencies for isolated deployment checks; do not copy source USD, Python
packages or private keys.

Repeated identical inputs/settings/tool versions yield identical prepared
payloads, provenance, build identities and packs when using the same key.
Changing source bytes (even authoring whitespace) changes the conservative build
identity. Provenance records source/settings hashes, cooker revision, Python,
cryptography/OpenSSL and OpenUSD versions. Record the vcpkg baseline, libsodium
revision, compiler and OS with runtime evidence. The initial implementation has
no Assimp, meshoptimizer, Slang or PhysX processing to report yet.

Recorded results and environment limitations are in the
[validation evidence](validation/content-pipeline.md). On case-sensitive WSL
storage, locally provided Windows CRT DLL filenames must match the PE import
names exactly.
