# Content pipeline

The Linux offline cooker converts a self-contained OpenUSD scene (`.usda` or
`.usdc`) into three signed packs: `.bfserver`, `.bfagent`, and `.bfclient`. It
verifies every generated pack before publishing the output directory.

Run the commands below from the **repository root**, using Bash in Linux or the
[development container](../../docs/development-container.md). On Windows, run
the cooker through WSL or the development container.

## Install dependencies

Use uv 0.10.4 or a reviewed compatible version. The Python version is pinned in
[.python-version](.python-version); [uv.lock](uv.lock) pins the runtime and
development dependencies, including OpenUSD.

```shell
uv sync --project tools/content_pipeline --locked --no-editable
uv run --project tools/content_pipeline --locked --no-sync cooker --help
```

## Generate signing keys

Choose a directory outside the checkout and runtime staging. This example uses a
local configuration directory; generate the pair once and reuse it for
subsequent cooks.

```shell
content_keys="${HOME}/.config/blackflower/content-keys"
mkdir -p "${content_keys}"
uv run --project tools/content_pipeline --locked --no-sync cooker keygen \
  --private-key "${content_keys}/signing.pem" \
  --public-key "${content_keys}/public.key"
```

Both destination files must be new and their parent directories must exist. The
private key is an unencrypted Ed25519 PEM/PKCS8 file with owner-only
permissions. Keep it in the signing environment. Give consumers the raw 32-byte
public key through an independently trusted channel.

## Cook the reference scene

The committed [reference scene](../../tests/integration/fixtures/mvp.usda)
provides a working input with collision shapes, lights, and spawn points. In the
same shell as the key setup, run:

```shell
uv run --project tools/content_pipeline --locked --no-sync cooker cook \
  --source tests/integration/fixtures/mvp.usda \
  --output build/packs/mvp \
  --private-key "${content_keys}/signing.pem"
```

The output directory must not already exist. Use a new destination, such as
`build/packs/mvp-next`, for another cook. Pack filenames use the source stem:

```text
build/packs/mvp/
├── mvp.bfserver
├── mvp.bfagent
└── mvp.bfclient
```

All three packs contain collision geometry and share a content build identity.
Only the server pack contains spawn points; only the client pack contains
lights. Success prints JSON on stdout. An interactive terminal also shows
progress on stderr; redirected stderr stays silent on success. Failures return a
nonzero exit status with a diagnostic.

## Prepare local source content

Choose a local authoring directory, which may be outside the checkout, and pass
the scene path to `--source`. The repository has no dedicated local asset
directory. For example, use `--source /path/to/scenes/mvp/mvp.usda`. Start from
the reference fixture and follow the
[OpenUSD authoring contract](../../schemas/scene/v1.md#openusd-authoring): Y-up
coordinates, explicit units, a `/Scenario` default prim with schema 1, and
optional `CollisionShapes`, `Lights`, and `Spawns` scopes. Omitted scopes
produce empty collections, including when all three are absent.

The current cooker supports boxes, spheres, point and directional lights, and
spawn points. GLB/glTF import, visual meshes, materials, textures, audio, and
shader compilation are not implemented. External USD references and asset
dependencies are also unsupported. A USDA containing only a `sourceAsset`
attribute pointing to a GLB is not a cookable scene.

## Verify with the C++ consumer

Build the native content harness using the [build guide](../../docs/build.md).
With a Debug build and the public key from the example above, run:

```shell
build/debug/blackflower_content_harness \
  build/packs/mvp/mvp.bfserver "${content_keys}/public.key"
build/debug/blackflower_content_harness \
  build/packs/mvp/mvp.bfagent "${content_keys}/public.key"
build/debug/blackflower_content_harness \
  build/packs/mvp/mvp.bfclient "${content_keys}/public.key"
```

The harness authenticates each pack and reports decoded scene values. This
checks content consumption; it does not render or run a simulation.

## Develop and validate

After changing Python package source, reinstall the non-editable package before
running it or its tests:

```shell
uv sync --project tools/content_pipeline --locked --no-editable \
  --reinstall-package cooker
```

With a configured and built native Debug tree, run the content checks:

```shell
ctest --preset debug -R '^content\.'
```

These include formatting, lint, type checks, and integration tests. See the
[Python guidelines](../../docs/python-guidelines.md#commands) for standalone
Pyink, Pylint, and mypy commands. For Markdown, JSON, shell, or JavaScript
changes, run:

```shell
npm --prefix tools/code_quality run check
```

## Troubleshooting and references

-   If dependencies or the `cooker` command are missing, rerun the installation
    command before using `--no-sync`.
-   If key generation reports an existing file, reuse the existing pair or
    choose new filenames.
-   If cooking reports an existing output directory, choose a new destination.
-   If a killed process leaves a sibling `.lock` file, confirm that no publisher
    is active before removing its stale lock or staging data.
-   If source validation fails, compare the scene with the committed fixture and
    the authoring contract. Valid OpenUSD syntax alone is insufficient.

See the [pipeline guide](../../docs/content-pipeline.md) for publication,
verification, and Windows harness details, the
[pack contract](../../schemas/pack/v1.md) for the binary format, and the
[validation evidence](../../docs/validation/content-pipeline.md) for recorded
results and environment limits.
