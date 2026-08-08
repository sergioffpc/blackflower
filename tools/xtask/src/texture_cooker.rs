use anyhow::{Context, bail};
use blackflower_assets::Bytes;
use blackflower_rendering_textures::{TextureMip, TextureSemantic, encode as encode_ktx2};
use glam::Vec3;
use half::f16;
use image::ImageFormat;

use crate::manifest::{LoadedAsset, TextureManifest, TextureSemanticManifest};
use crate::profile::TextureProfile;

const MAX_TEXTURE_DIMENSION: u32 = 16_384;

pub(crate) fn cook(
    source: &LoadedAsset,
    manifest: &TextureManifest,
    profile: TextureProfile,
) -> anyhow::Result<Bytes> {
    let semantic = texture_semantic(manifest.semantic);
    let format = match semantic {
        TextureSemantic::HdrLinear => ImageFormat::OpenExr,
        TextureSemantic::ColorSrgb
        | TextureSemantic::NormalLinear
        | TextureSemantic::DataLinear => ImageFormat::Png,
    };
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_TEXTURE_DIMENSION);
    limits.max_image_height = Some(MAX_TEXTURE_DIMENSION);
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(&source.source_bytes), format);
    reader.limits(limits);
    let image = reader
        .decode()
        .context("image decoder rejected texture source")?;
    validate_dimensions(image.width(), image.height())?;

    let levels = match semantic {
        TextureSemantic::HdrLinear => hdr_levels(image),
        TextureSemantic::ColorSrgb
        | TextureSemantic::NormalLinear
        | TextureSemantic::DataLinear => ldr_levels(image, semantic),
    }?;
    let borrowed = levels
        .iter()
        .map(|level| TextureMip {
            width: level.width,
            height: level.height,
            bytes: &level.bytes,
        })
        .collect::<Vec<_>>();
    encode_ktx2(semantic, &borrowed, profile.encode_options()?)
        .context("KTX-Software rejected canonical texture mips")
}

fn validate_dimensions(width: u32, height: u32) -> anyhow::Result<()> {
    if width == 0 || height == 0 {
        bail!("texture dimensions must be non-zero");
    }
    if width > MAX_TEXTURE_DIMENSION || height > MAX_TEXTURE_DIMENSION {
        bail!(
            "texture dimensions {width}x{height} exceed the {MAX_TEXTURE_DIMENSION} dimension limit"
        );
    }
    Ok(())
}

fn ldr_levels(
    image: image::DynamicImage,
    semantic: TextureSemantic,
) -> anyhow::Result<Vec<EncodedLevel>> {
    let image = image.to_rgba8();
    let pixels = image
        .pixels()
        .map(|pixel| ldr_pixel(pixel.0, semantic))
        .collect();
    encode_levels(
        FloatLevel {
            width: image.width(),
            height: image.height(),
            pixels,
        },
        semantic,
    )
}

fn hdr_levels(image: image::DynamicImage) -> anyhow::Result<Vec<EncodedLevel>> {
    let image = image.to_rgba32f();
    let mut pixels = Vec::with_capacity(image.pixels().len());
    for pixel in image.pixels() {
        if pixel
            .0
            .iter()
            .any(|value| !value.is_finite() || value.abs() > 65_504.0)
        {
            bail!("HDR texture contains a channel outside the finite binary16 range");
        }
        pixels.push(pixel.0);
    }
    encode_levels(
        FloatLevel {
            width: image.width(),
            height: image.height(),
            pixels,
        },
        TextureSemantic::HdrLinear,
    )
}

fn encode_levels(
    mut level: FloatLevel,
    semantic: TextureSemantic,
) -> anyhow::Result<Vec<EncodedLevel>> {
    let mut output = Vec::new();
    loop {
        output.push(EncodedLevel {
            width: level.width,
            height: level.height,
            bytes: encode_pixels(&level.pixels, semantic),
        });
        if level.width == 1 && level.height == 1 {
            return Ok(output);
        }
        level = downsample(&level, semantic)?;
    }
}

fn downsample(source: &FloatLevel, semantic: TextureSemantic) -> anyhow::Result<FloatLevel> {
    let width = (source.width / 2).max(1);
    let height = (source.height / 2).max(1);
    let capacity = usize::try_from(width)
        .ok()
        .and_then(|value| {
            usize::try_from(height)
                .ok()
                .and_then(|height| value.checked_mul(height))
        })
        .context("mip dimensions exceed addressable memory")?;
    let mut pixels = Vec::with_capacity(capacity);
    for y in 0..height {
        for x in 0..width {
            let mut value = average_region(source, x, y, width, height)?;
            if semantic == TextureSemantic::NormalLinear {
                value = normalized_normal(value);
            }
            pixels.push(value);
        }
    }
    Ok(FloatLevel {
        width,
        height,
        pixels,
    })
}

fn average_region(
    source: &FloatLevel,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> anyhow::Result<[f32; 4]> {
    let (start_x, end_x) = source_range(x, width, source.width);
    let (start_y, end_y) = source_range(y, height, source.height);
    let mut sum = [0.0_f32; 4];
    let mut count = 0_u32;
    for source_y in start_y..end_y {
        for source_x in start_x..end_x {
            let index = pixel_index(source_x, source_y, source.width)?;
            for (destination, channel) in sum.iter_mut().zip(source.pixels[index]) {
                *destination += channel;
            }
            count += 1;
        }
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "one destination texel samples at most nine source texels"
    )]
    let divisor = count as f32;
    Ok(sum.map(|value| value / divisor))
}

fn source_range(index: u32, destination_size: u32, source_size: u32) -> (u32, u32) {
    let index = u64::from(index);
    let destination_size = u64::from(destination_size);
    let source_size = u64::from(source_size);
    let start = index * source_size / destination_size;
    let end = (index + 1) * source_size / destination_size;
    (
        u32::try_from(start).unwrap_or(u32::MAX),
        u32::try_from(end).unwrap_or(u32::MAX),
    )
}

fn pixel_index(x: u32, y: u32, width: u32) -> anyhow::Result<usize> {
    usize::try_from(u64::from(y) * u64::from(width) + u64::from(x))
        .context("pixel index exceeds addressable memory")
}

fn ldr_pixel(pixel: [u8; 4], semantic: TextureSemantic) -> [f32; 4] {
    let unit = pixel.map(|value| f32::from(value) / 255.0);
    if semantic == TextureSemantic::ColorSrgb {
        [
            srgb_to_linear(unit[0]),
            srgb_to_linear(unit[1]),
            srgb_to_linear(unit[2]),
            unit[3],
        ]
    } else if semantic == TextureSemantic::NormalLinear {
        [
            unit[0].mul_add(2.0, -1.0),
            unit[1].mul_add(2.0, -1.0),
            unit[2].mul_add(2.0, -1.0),
            unit[3],
        ]
    } else {
        unit
    }
}

fn encode_pixels(pixels: &[[f32; 4]], semantic: TextureSemantic) -> Vec<u8> {
    match semantic {
        TextureSemantic::HdrLinear => pixels
            .iter()
            .flat_map(|pixel| {
                pixel
                    .iter()
                    .flat_map(|value| f16::from_f32(*value).to_le_bytes())
            })
            .collect(),
        TextureSemantic::ColorSrgb => pixels
            .iter()
            .flat_map(|pixel| {
                [
                    unit_to_u8(linear_to_srgb(pixel[0])),
                    unit_to_u8(linear_to_srgb(pixel[1])),
                    unit_to_u8(linear_to_srgb(pixel[2])),
                    unit_to_u8(pixel[3]),
                ]
            })
            .collect(),
        TextureSemantic::NormalLinear => pixels
            .iter()
            .flat_map(|pixel| {
                [
                    signed_to_u8(pixel[0]),
                    signed_to_u8(pixel[1]),
                    signed_to_u8(pixel[2]),
                    unit_to_u8(pixel[3]),
                ]
            })
            .collect(),
        TextureSemantic::DataLinear => pixels
            .iter()
            .flat_map(|pixel| pixel.map(unit_to_u8))
            .collect(),
    }
}

fn normalized_normal(value: [f32; 4]) -> [f32; 4] {
    let normal = Vec3::from_array([value[0], value[1], value[2]]).normalize_or(Vec3::Z);
    [normal.x, normal.y, normal.z, value[3]]
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the channel is clamped and rounded to the complete u8 range before conversion"
)]
fn unit_to_u8(value: f32) -> u8 {
    value.clamp(0.0, 1.0).mul_add(255.0, 0.0).round() as u8
}

fn signed_to_u8(value: f32) -> u8 {
    unit_to_u8(value.mul_add(0.5, 0.5))
}

fn texture_semantic(value: TextureSemanticManifest) -> TextureSemantic {
    match value {
        TextureSemanticManifest::ColorSrgb => TextureSemantic::ColorSrgb,
        TextureSemanticManifest::NormalLinear => TextureSemantic::NormalLinear,
        TextureSemanticManifest::DataLinear => TextureSemantic::DataLinear,
        TextureSemanticManifest::HdrLinear => TextureSemantic::HdrLinear,
    }
}

struct FloatLevel {
    width: u32,
    height: u32,
    pixels: Vec<[f32; 4]>,
}

struct EncodedLevel {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}
