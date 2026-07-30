# blackflower-rendering-textures

Safe Rust boundary around the pinned
[KTX-Software 4.4.2](https://github.com/KhronosGroup/KTX-Software/releases/tag/v4.4.2)
source. The crate creates and validates KTX2 textures and transcodes UASTC
payloads into adapter-supported runtime formats.

The native dependency is built statically. Raw declarations stay private;
callers use `TextureAsset`, `TextureTargetCapabilities`, and
`TranscodedTexture`.

The encoder always uses one BasisU worker and disables RDO multithreading.
This makes repeated cooks stable on one toolchain platform. KTX-Software
documents that BasisU output is not bit-identical across platforms, so release
packages must be cooked on the designated canonical host. Runtime loading and
transcoding remain portable across Linux, macOS, and Windows.

```rust
use blackflower_rendering_textures::{
    TextureAsset, TextureTargetCapabilities,
};
use bytes::Bytes;

# fn example(cooked: Bytes) -> Result<(), Box<dyn std::error::Error>> {
let texture = TextureAsset::from_bytes(cooked)?;
let upload = texture.transcode(TextureTargetCapabilities {
    bc: true,
    astc: false,
    etc2: false,
})?;
assert!(!upload.bytes.is_empty());
# Ok(())
# }
```
