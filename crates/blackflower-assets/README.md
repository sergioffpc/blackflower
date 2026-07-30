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

`PackagePayloadHash` identifies the authenticated SquashFS bytes.
`PackageHash` continues to cover every final file byte, including the signing
footer, so changing the signer or signature changes `AssetSetHash`.

Package names are lowercase ASCII and use ordinary bytewise ordering. Use
zero-padded names such as `pak000.squashfs`, `pak100-expansion.squashfs`, and
`pak900-hotfix.squashfs`; this ordering is deliberately not natural numeric
ordering.

The first pipeline stage supports opaque blobs only. Domain-specific cookers
for textures, shaders, models, animation, volumes, navigation, and audio add
new `AssetKind` variants without changing the package overlay contract.

## Runtime example

```rust,no_run
use blackflower_assets::{AssetId, AssetStore, AssetTrustStore};
use std::str::FromStr;

# fn example(release_asset_public_key: [u8; 32]) -> Result<(), Box<dyn std::error::Error>> {
let trusted_keys =
    AssetTrustStore::from_public_keys([release_asset_public_key])?;
let store = AssetStore::open_dir(
    "target/assets/packages/desktop-universal",
    &trusted_keys,
)?;
let id = AssetId::from_str("fixtures/example")?;
let bytes = store.read_asset(&id)?;
assert!(!bytes.is_empty());
# Ok(())
# }
```
