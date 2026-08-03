use bytes::Bytes;

use super::{
    EncodeOptions, TextureAsset, TextureFormat, TextureMip, TextureQuality, TextureSemantic,
    TextureTargetCapabilities, encode, ktx_version,
};

fn fixture_levels() -> Vec<Vec<u8>> {
    vec![
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ],
        vec![128, 128, 128, 255],
    ]
}

#[test]
fn cooks_and_transcodes_uastc() -> Result<(), crate::Error> {
    let owned = fixture_levels();
    let mips = [
        TextureMip {
            width: 2,
            height: 2,
            bytes: &owned[0],
        },
        TextureMip {
            width: 1,
            height: 1,
            bytes: &owned[1],
        },
    ];
    let bytes = encode(
        TextureSemantic::ColorSrgb,
        &mips,
        EncodeOptions {
            quality: TextureQuality::Fast,
            uastc_rdo: false,
            zstd_level: 3,
        },
    )?;
    let asset = TextureAsset::from_bytes(bytes)?;
    assert_eq!(asset.dimensions(), (2, 2));
    assert_eq!(asset.level_count(), 2);
    assert_eq!(asset.semantic(), TextureSemantic::ColorSrgb);

    let transcoded = asset.transcode(TextureTargetCapabilities {
        bc: true,
        astc: false,
        etc2: false,
    })?;
    assert_eq!(transcoded.format, TextureFormat::Bc7Rgba);
    assert_eq!(transcoded.levels.len(), 2);
    assert!(!transcoded.bytes.is_empty());
    assert_eq!(ktx_version(), "4.4.2");
    Ok(())
}

#[test]
fn high_quality_uastc_is_stable_with_single_threaded_rdo() -> Result<(), crate::Error> {
    let owned = fixture_levels();
    let mips = [
        TextureMip {
            width: 2,
            height: 2,
            bytes: &owned[0],
        },
        TextureMip {
            width: 1,
            height: 1,
            bytes: &owned[1],
        },
    ];
    let options = EncodeOptions {
        quality: TextureQuality::High,
        uastc_rdo: true,
        zstd_level: 15,
    };
    let first = encode(TextureSemantic::ColorSrgb, &mips, options)?;
    let second = encode(TextureSemantic::ColorSrgb, &mips, options)?;
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn cooks_hdr_as_upload_ready_rgba16f() -> Result<(), crate::Error> {
    let texel = [0x00, 0x3c, 0x00, 0x38, 0x00, 0x34, 0x00, 0x3c];
    let base = texel.repeat(4);
    let smallest = texel;
    let mips = [
        TextureMip {
            width: 2,
            height: 2,
            bytes: &base,
        },
        TextureMip {
            width: 1,
            height: 1,
            bytes: &smallest,
        },
    ];
    let bytes = encode(
        TextureSemantic::HdrLinear,
        &mips,
        EncodeOptions {
            quality: TextureQuality::Fast,
            uastc_rdo: false,
            zstd_level: 3,
        },
    )?;
    let asset = TextureAsset::from_bytes(bytes)?;
    let upload = asset.transcode(TextureTargetCapabilities::default())?;
    assert_eq!(upload.format, TextureFormat::Rgba16Float);
    assert_eq!(upload.levels.len(), 2);
    assert_eq!(upload.bytes.len(), base.len() + smallest.len());
    Ok(())
}

#[test]
fn selects_formats_from_semantics_and_capabilities() {
    assert_eq!(
        TextureTargetCapabilities {
            bc: true,
            astc: false,
            etc2: false,
        }
        .select(TextureSemantic::NormalLinear),
        TextureFormat::Bc5Rg
    );
    assert_eq!(
        TextureTargetCapabilities {
            bc: false,
            astc: true,
            etc2: false,
        }
        .select(TextureSemantic::DataLinear),
        TextureFormat::Astc4x4Rgba
    );
    assert_eq!(
        TextureTargetCapabilities {
            bc: false,
            astc: false,
            etc2: true,
        }
        .select(TextureSemantic::NormalLinear),
        TextureFormat::Etc2EacRg11
    );
}

#[test]
fn rejects_incomplete_mip_chain() {
    let bytes = [0_u8; 4 * 4 * 4];
    let error = encode(
        TextureSemantic::DataLinear,
        &[TextureMip {
            width: 4,
            height: 4,
            bytes: &bytes,
        }],
        EncodeOptions {
            quality: TextureQuality::Fast,
            uastc_rdo: false,
            zstd_level: 3,
        },
    );
    assert!(error.is_err());
}

#[test]
fn rejects_non_ktx2_asset() {
    assert!(TextureAsset::from_bytes(Bytes::from_static(b"not KTX2")).is_err());
}
