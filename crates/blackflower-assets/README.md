# blackflower-assets

`blackflower-assets` owns Blackflower's cooked-content identity and read-only
runtime VFS. Cooked assets live in deterministic SquashFS 4.0 packages.
Applications open every package in one directory and resolve duplicate asset
identifiers from the lexicographically greatest package name to the smallest.
This gives later packages Quake-style override semantics without extracting or
mounting their contents.

Every package ends in a fixed Blackflower signature footer. The footer records
the BLAKE3 digest of the exact SquashFS payload and an Ed25519 signature over
that 32-byte digest. The executable supplies the trusted public keys through
`AssetTrustStore`; a key shipped inside a package is never trusted. Unsigned
packages, unknown keys, altered payloads, and invalid signatures fail before
the embedded catalog is accepted.

The runtime loads package metadata and catalogs at startup. Asset bytes remain
inside SquashFS and are decompressed on demand. Each reader verifies the
catalogued byte length and BLAKE3 content hash when it reaches the end of the
asset. Complete reads return `Bytes`, allowing cheap clones and slices after
the verified object has been materialized.

Consumers that cross into native parsers which are not memory-safe verifiers
must instead call `read_authenticated_asset`. Its opaque `AuthenticatedAsset`
retains the signed catalog kind, cooking profile, toolchain, package hashes,
and signing-key identity alongside the fully hash-checked bytes. This proof
lets a backend such as Luau require signed, correctly typed content without
exposing a safe raw-byte constructor at the native loading boundary.

`PackagePayloadHash` identifies the authenticated SquashFS bytes.
`PackageHash` continues to cover every final file byte, including the signing
footer, so changing the signer or signature changes `AssetSetHash`.

Package names are lowercase ASCII and use ordinary bytewise ordering. Use
zero-padded names such as `pak000.squashfs`, `pak100-expansion.squashfs`, and
`pak900-hotfix.squashfs`; this ordering is deliberately not natural numeric
ordering.

Each package records the portable name and canonical BLAKE3 identity of its
cooking profile. A layered store rejects packages with different profile names
or hashes before resolving any assets. Profile definitions live in
`assets/profiles`; individual asset manifests cannot override their settings.
The profile identity, runtime asset kinds, and complete cooker toolchain
identity remain under the unreleased catalog schema 1. Development changes do
not advance this schema; the release process owns version changes.

The pipeline supports opaque blobs, profile-configured Luau bytecode,
Naga-validated SPIR-V compiled from Slang, and semantic KTX2 textures cooked
from PNG or OpenEXR. It also supports meshoptimizer-optimized static meshes
with generated LOD chains, `.bfmodel` scene hierarchies with typed Mesh and
Volume attachments, uncompressed NanoVDB volumes, `.bfskel` skeletons, and
one-clip `.bfanim` assets cooked from glTF or GLB. Animation records carry
their skeleton `AssetId` as a typed catalog dependency. Model records carry
their Mesh and Volume attachment IDs as typed dependencies. Package selection
closes over both relationships before cooking. It also supports tiled
`.bfnav`, 48 kHz PCM16 `.bfaudio`, lossless 48 kHz FLAC streams, and source-less
`.bfsound` event policy. Sound-event records carry their media `AssetId` as a
typed dependency, so package selection and recipe identity close over audio
references. Acoustics adds schema-1 `.bfacscn` scenes, `.bfacprb` probe batches,
and `.bfac` zone descriptors: probes depend on their scene, and the descriptor
depends on scenes, probes, and `.bfactpl` topology. The shared `.bfacmat`,
`.bfactpl`, and `.bfacpfb` contracts are joined by the simulation-only
`.bfacsim` and `.bfacprf`. An emission profile reads its audio media only while
cooking and therefore does not force WAV, PCM, FLAC, `.bfaudio`, or
`.bfsound` into a server package. These domain kinds do not change the package
overlay contract.

## Optional hot reload

The `hot-reload` Cargo feature is disabled by default. Enabling it adds
`AssetStoreManager` and `AssetStoreWatcher`; builds without the feature do not
compile the filesystem-watcher dependency or expose hot-reload APIs.

`AssetStoreManager` owns the currently published immutable snapshot. An
explicit `reload` opens every package again, authenticates signatures,
validates the complete overlay, and compares winning records without blocking
readers of the current snapshot. A changed candidate is published with one
short state swap. Any failure leaves the previous snapshot and generation
untouched.

Snapshots are reference counted. Systems that already hold a snapshot continue
to read its package files after a reload, while new snapshots observe the new
package set. Reload reports changes in `AssetId` order and preserves each
asset's `AssetKind` and `AssetAudience`. Reclassifying an existing ID is
rejected and requires a fresh manager.

`AssetStoreWatcher` observes only direct children of the package directory and
coalesces native notifications during a caller-selected debounce window. Only
paths with the exact `.squashfs` extension request a reload. Successful reloads,
rejected candidates, and watcher failures are delivered through a channel;
there are no callbacks into simulation or presentation systems.

Presentation changes can be adopted at a frame boundary. Simulation changes
should be adopted only at a deterministic tick, level, or session boundary.
Shared changes use the stricter boundary of their consumers. Production
executables can omit the feature entirely.

```rust,no_run
# #[cfg(feature = "hot-reload")]
# mod hot_reload_example {
use blackflower_assets::{
    AssetAudience, AssetReloadStatus, AssetStoreManager, AssetStoreWatcher,
    AssetTrustStore, AssetWatchEvent,
};
use std::sync::Arc;
use std::time::Duration;

# fn example(release_asset_public_key: [u8; 32]) -> Result<(), Box<dyn std::error::Error>> {
let trusted_keys =
    AssetTrustStore::from_public_keys([release_asset_public_key])?;
let manager = Arc::new(AssetStoreManager::open_dir(
    "target/assets/packages/release",
    trusted_keys,
)?);
let watcher =
    AssetStoreWatcher::watch(Arc::clone(&manager), Duration::from_millis(150))?;

if let AssetWatchEvent::Reloaded(reload) = watcher.events().recv()? {
    if reload.status() == AssetReloadStatus::Reloaded {
        for change in reload.changes().changes() {
            match change.audience() {
                AssetAudience::Presentation => {
                    // Adopt at a presentation frame boundary.
                }
                AssetAudience::Simulation | AssetAudience::Shared => {
                    // Defer to an approved deterministic boundary.
                }
            }
        }
    }
}
# Ok(())
# }
# }
```

## Runtime example

```rust,no_run
use blackflower_assets::{AssetId, AssetStore, AssetTrustStore};
use std::str::FromStr;

# fn example(release_asset_public_key: [u8; 32]) -> Result<(), Box<dyn std::error::Error>> {
let trusted_keys =
    AssetTrustStore::from_public_keys([release_asset_public_key])?;
let store = AssetStore::open_dir(
    "target/assets/packages/release",
    &trusted_keys,
)?;
let id = AssetId::from_str("fixtures/example")?;
let bytes = store.read_asset(&id)?;
assert!(!bytes.is_empty());
# Ok(())
# }
```
