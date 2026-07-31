use super::{GLB_HEADER_BYTES, JSON_CHUNK, json_bytes};
use crate::Error;

#[test]
fn plain_json_passes_through() -> Result<(), Error> {
    let bytes = br#"{"asset":{"version":"2.0"}}"#;
    assert_eq!(json_bytes(bytes)?, bytes);
    Ok(())
}

#[test]
fn glb_exposes_its_json_chunk() -> Result<(), Box<dyn std::error::Error>> {
    let json = br#"{"asset":{"version":"2.0"}}"#;
    let glb = build_glb(json)?;
    assert_eq!(json_bytes(&glb)?, padded_json(json));
    Ok(())
}

#[test]
fn glb_declared_length_is_exact() -> Result<(), Box<dyn std::error::Error>> {
    let mut glb = build_glb(br#"{"asset":{"version":"2.0"}}"#)?;
    let declared = u32::try_from(glb.len())?.saturating_add(4);
    glb[8..12].copy_from_slice(&declared.to_le_bytes());
    assert!(matches!(
        json_bytes(&glb),
        Err(Error::GlbLengthMismatch { .. })
    ));
    Ok(())
}

fn build_glb(json: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let json = padded_json(json);
    let total = GLB_HEADER_BYTES + 8 + json.len();
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(b"glTF");
    output.extend_from_slice(&2_u32.to_le_bytes());
    output.extend_from_slice(&u32::try_from(total)?.to_le_bytes());
    output.extend_from_slice(&u32::try_from(json.len())?.to_le_bytes());
    output.extend_from_slice(&JSON_CHUNK.to_le_bytes());
    output.extend_from_slice(&json);
    Ok(output)
}

fn padded_json(json: &[u8]) -> Vec<u8> {
    let mut output = json.to_vec();
    while !output.len().is_multiple_of(4) {
        output.push(b' ');
    }
    output
}
