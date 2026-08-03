use bytes::Bytes;

use crate::{Error, ffi};

const KTX2_IDENTIFIER: [u8; 12] = [
    0xab, b'K', b'T', b'X', b' ', b'2', b'0', 0xbb, 0x0d, 0x0a, 0x1a, 0x0a,
];

/// Meaning assigned to the texels of one authored texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TextureSemantic {
    /// Color sampled with the sRGB transfer function.
    ColorSrgb = 1,
    /// Linear tangent-space normal vectors.
    NormalLinear = 2,
    /// Linear non-color channels such as masks or scalar material data.
    DataLinear = 3,
    /// Linear high-dynamic-range color.
    HdrLinear = 4,
}

impl TextureSemantic {
    pub(crate) const fn raw(self) -> i32 {
        self as i32
    }

    pub(crate) fn from_raw(value: i32) -> Result<Self, Error> {
        match value {
            1 => Ok(Self::ColorSrgb),
            2 => Ok(Self::NormalLinear),
            3 => Ok(Self::DataLinear),
            4 => Ok(Self::HdrLinear),
            _ => Err(Error::InvalidKtx2(format!(
                "unknown texture semantic {value}"
            ))),
        }
    }

    const fn bytes_per_texel(self) -> usize {
        match self {
            Self::ColorSrgb | Self::NormalLinear | Self::DataLinear => 4,
            Self::HdrLinear => 8,
        }
    }
}

/// UASTC encoding effort selected by a complete cooking profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TextureQuality {
    /// Prefer short development cook times.
    Fast = 1,
    /// Prefer release image quality.
    High = 2,
}

impl TextureQuality {
    pub(crate) const fn raw(self) -> i32 {
        self as i32
    }
}

/// Profile-owned encoder settings for one texture cook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    /// UASTC encoding effort for LDR textures.
    pub quality: TextureQuality,
    /// Enable the deterministic single-threaded UASTC RDO pass.
    pub uastc_rdo: bool,
    /// Zstandard level from 1 through 22.
    pub zstd_level: u32,
}

/// One canonical source mip passed to KTX-Software.
#[derive(Debug, Clone, Copy)]
pub struct TextureMip<'a> {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Tightly packed RGBA8 or little-endian RGBA16F bytes.
    pub bytes: &'a [u8],
}

/// Encode a complete two-dimensional mip chain as KTX2.
///
/// # Errors
///
/// Returns an error for invalid dimensions, incomplete mip chains, incorrect
/// byte lengths, unsupported settings, or native encoder failures.
pub fn encode(
    semantic: TextureSemantic,
    levels: &[TextureMip<'_>],
    options: EncodeOptions,
) -> Result<Bytes, Error> {
    validate_levels(semantic, levels)?;
    if !(1..=22).contains(&options.zstd_level) {
        return Err(Error::InvalidInput(
            "Zstandard level must be from 1 through 22".to_owned(),
        ));
    }
    let bytes = ffi::encode(levels, semantic, options)?;
    if !bytes.starts_with(&KTX2_IDENTIFIER) {
        return Err(Error::InvalidKtx2(
            "native encoder did not return a KTX2 container".to_owned(),
        ));
    }
    let info = ffi::inspect(&bytes)?;
    if info.width != levels[0].width
        || info.height != levels[0].height
        || usize::try_from(info.levels).ok() != Some(levels.len())
        || info.semantic != semantic
    {
        return Err(Error::InvalidKtx2(
            "encoded KTX2 metadata differs from the source contract".to_owned(),
        ));
    }
    Ok(bytes)
}

/// Validated immutable KTX2 texture loaded from the asset VFS.
#[derive(Debug, Clone)]
pub struct TextureAsset {
    bytes: Bytes,
    semantic: TextureSemantic,
    width: u32,
    height: u32,
    levels: u32,
}

impl TextureAsset {
    /// Validate authenticated KTX2 bytes and retain them without copying.
    ///
    /// # Errors
    ///
    /// Returns an error when the bytes are not a supported Blackflower KTX2
    /// texture or required semantic metadata is missing.
    pub fn from_bytes(bytes: Bytes) -> Result<Self, Error> {
        if !bytes.starts_with(&KTX2_IDENTIFIER) {
            return Err(Error::InvalidKtx2(
                "asset does not start with the KTX2 identifier".to_owned(),
            ));
        }
        let info = ffi::inspect(&bytes)?;
        Ok(Self {
            bytes,
            semantic: info.semantic,
            width: info.width,
            height: info.height,
            levels: info.levels,
        })
    }

    /// Authored texel meaning.
    #[must_use]
    pub const fn semantic(&self) -> TextureSemantic {
        self.semantic
    }

    /// Base mip dimensions.
    #[must_use]
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Number of mip levels.
    #[must_use]
    pub const fn level_count(&self) -> u32 {
        self.levels
    }

    /// Original validated KTX2 bytes.
    #[must_use]
    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// Select a supported GPU representation and produce upload-ready levels.
    ///
    /// # Errors
    ///
    /// Returns an error if KTX-Software rejects the selected transcode.
    pub fn transcode(
        &self,
        capabilities: TextureTargetCapabilities,
    ) -> Result<TranscodedTexture, Error> {
        let target = capabilities.select(self.semantic);
        ffi::transcode(&self.bytes, target)
    }
}

/// Runtime texture compression capabilities supplied by the renderer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TextureTargetCapabilities {
    /// BC formats such as BC7 and BC5 are supported.
    pub bc: bool,
    /// ASTC 4x4 LDR is supported.
    pub astc: bool,
    /// ETC2 and EAC are supported.
    pub etc2: bool,
}

impl TextureTargetCapabilities {
    const fn select(self, semantic: TextureSemantic) -> TextureFormat {
        match semantic {
            TextureSemantic::HdrLinear => TextureFormat::Rgba16Float,
            TextureSemantic::NormalLinear if self.bc => TextureFormat::Bc5Rg,
            TextureSemantic::NormalLinear if self.astc => TextureFormat::Astc4x4Rgba,
            TextureSemantic::NormalLinear if self.etc2 => TextureFormat::Etc2EacRg11,
            TextureSemantic::ColorSrgb | TextureSemantic::DataLinear if self.bc => {
                TextureFormat::Bc7Rgba
            }
            TextureSemantic::ColorSrgb | TextureSemantic::DataLinear if self.astc => {
                TextureFormat::Astc4x4Rgba
            }
            TextureSemantic::ColorSrgb | TextureSemantic::DataLinear if self.etc2 => {
                TextureFormat::Etc2Rgba
            }
            TextureSemantic::ColorSrgb
            | TextureSemantic::NormalLinear
            | TextureSemantic::DataLinear => TextureFormat::Rgba8,
        }
    }
}

/// Concrete upload format selected for a runtime adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TextureFormat {
    /// Four 8-bit normalized channels.
    Rgba8 = 1,
    /// Four IEEE-754 binary16 channels.
    Rgba16Float = 2,
    /// BC7 RGBA blocks.
    Bc7Rgba = 3,
    /// BC5 two-channel blocks, suitable for normal X/Y.
    Bc5Rg = 4,
    /// ASTC 4x4 RGBA blocks.
    Astc4x4Rgba = 5,
    /// ETC2 RGBA blocks.
    Etc2Rgba = 6,
    /// ETC2 EAC two-channel blocks.
    Etc2EacRg11 = 7,
}

impl TextureFormat {
    pub(crate) const fn raw(self) -> i32 {
        self as i32
    }

    pub(crate) fn from_raw(value: i32) -> Result<Self, Error> {
        match value {
            1 => Ok(Self::Rgba8),
            2 => Ok(Self::Rgba16Float),
            3 => Ok(Self::Bc7Rgba),
            4 => Ok(Self::Bc5Rg),
            5 => Ok(Self::Astc4x4Rgba),
            6 => Ok(Self::Etc2Rgba),
            7 => Ok(Self::Etc2EacRg11),
            _ => Err(Error::InvalidKtx2(format!(
                "native transcoder returned unknown format {value}"
            ))),
        }
    }
}

/// One mip's placement inside [`TranscodedTexture::bytes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscodedMip {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Byte offset in the complete payload.
    pub offset: usize,
    /// Number of bytes occupied by this mip.
    pub byte_len: usize,
}

/// Capability-selected texture bytes ready for renderer upload.
#[derive(Debug, Clone)]
pub struct TranscodedTexture {
    /// Concrete GPU or fallback format.
    pub format: TextureFormat,
    /// Authored texel meaning.
    pub semantic: TextureSemantic,
    /// Base width.
    pub width: u32,
    /// Base height.
    pub height: u32,
    /// Ordered mip layouts.
    pub levels: Vec<TranscodedMip>,
    /// Concatenated mip bytes.
    pub bytes: Bytes,
}

/// Returns the exact pinned KTX-Software release used by this crate.
#[must_use]
pub fn ktx_version() -> &'static str {
    ffi::ktx_version()
}

fn validate_levels(semantic: TextureSemantic, levels: &[TextureMip<'_>]) -> Result<(), Error> {
    let first = levels
        .first()
        .ok_or_else(|| Error::InvalidInput("texture must contain at least one mip".to_owned()))?;
    if first.width == 0 || first.height == 0 {
        return Err(Error::InvalidInput(
            "texture dimensions must be non-zero".to_owned(),
        ));
    }
    let expected_levels = mip_count(first.width, first.height);
    if levels.len() != expected_levels {
        return Err(Error::InvalidInput(format!(
            "complete mip chain requires {expected_levels} levels, received {}",
            levels.len()
        )));
    }
    for (index, level) in levels.iter().enumerate() {
        let shift = u32::try_from(index)
            .map_err(|_error| Error::InvalidInput("mip index exceeds u32".to_owned()))?;
        let expected_width = first.width.checked_shr(shift).unwrap_or(0).max(1);
        let expected_height = first.height.checked_shr(shift).unwrap_or(0).max(1);
        if level.width != expected_width || level.height != expected_height {
            return Err(Error::InvalidInput(format!(
                "mip {index} must be {expected_width}x{expected_height}, received {}x{}",
                level.width, level.height
            )));
        }
        let texels = usize::try_from(level.width)
            .ok()
            .and_then(|width| {
                usize::try_from(level.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|count| count.checked_mul(semantic.bytes_per_texel()))
            .ok_or_else(|| Error::InvalidInput(format!("mip {index} byte length overflows")))?;
        if level.bytes.len() != texels {
            return Err(Error::InvalidInput(format!(
                "mip {index} requires {texels} bytes, received {}",
                level.bytes.len()
            )));
        }
    }
    Ok(())
}

fn mip_count(width: u32, height: u32) -> usize {
    usize::try_from(u32::BITS - width.max(height).leading_zeros()).unwrap_or(usize::MAX)
}

#[cfg(test)]
#[path = "../tests/unit/texture.rs"]
mod tests;
