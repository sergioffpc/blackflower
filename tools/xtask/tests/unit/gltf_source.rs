use std::fs;

use tempfile::TempDir;

use super::validate_tree;

#[test]
fn validates_every_gltf_and_glb_source() -> anyhow::Result<()> {
    let directory = TempDir::new()?;
    let nested = directory.path().join("models");
    fs::create_dir(&nested)?;
    fs::write(nested.join("first.gltf"), br#"{"asset":{"version":"2.0"}}"#)?;
    fs::write(
        nested.join("second.glb"),
        minimal_glb(br#"{"asset":{"version":"2.0"}}"#)?,
    )?;
    fs::write(
        nested.join("ignored.json"),
        br#"{"asset":{"version":"1.0"}}"#,
    )?;

    let summary = validate_tree(directory.path())?;
    assert_eq!(summary.sources, 2);
    Ok(())
}

#[test]
fn rejects_invalid_gltf_before_cooking() -> anyhow::Result<()> {
    let directory = TempDir::new()?;
    fs::write(
        directory.path().join("invalid.gltf"),
        br#"{"asset":{"version":"1.0"}}"#,
    )?;

    assert!(validate_tree(directory.path()).is_err());
    Ok(())
}

fn minimal_glb(json: &[u8]) -> anyhow::Result<Vec<u8>> {
    const HEADER_BYTES: usize = 12;
    const CHUNK_HEADER_BYTES: usize = 8;
    const JSON_CHUNK: u32 = 0x4e4f_534a;

    let mut json = json.to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let total = HEADER_BYTES + CHUNK_HEADER_BYTES + json.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2_u32.to_le_bytes());
    glb.extend_from_slice(&u32::try_from(total)?.to_le_bytes());
    glb.extend_from_slice(&u32::try_from(json.len())?.to_le_bytes());
    glb.extend_from_slice(&JSON_CHUNK.to_le_bytes());
    glb.extend_from_slice(&json);
    Ok(glb)
}
