use crate::Error;

const GLB_MAGIC: &[u8; 4] = b"glTF";
const GLB_VERSION: u32 = 2;
const GLB_HEADER_BYTES: usize = 12;
const CHUNK_HEADER_BYTES: usize = 8;
const JSON_CHUNK: u32 = 0x4e4f_534a;

pub(crate) fn json_bytes(bytes: &[u8]) -> Result<&[u8], Error> {
    if bytes.get(..GLB_MAGIC.len()) != Some(GLB_MAGIC) {
        return Ok(bytes);
    }
    parse_glb(bytes)
}

fn parse_glb(bytes: &[u8]) -> Result<&[u8], Error> {
    if bytes.len() < GLB_HEADER_BYTES {
        return Err(Error::TruncatedGlb);
    }
    let version = read_u32(bytes, 4)?;
    if version != GLB_VERSION {
        return Err(Error::UnsupportedGlbVersion(version));
    }
    let declared = usize::try_from(read_u32(bytes, 8)?).map_err(|_error| Error::TruncatedGlb)?;
    if declared != bytes.len() {
        return Err(Error::GlbLengthMismatch {
            declared,
            actual: bytes.len(),
        });
    }

    let (json, next) = chunk(bytes, GLB_HEADER_BYTES)?;
    if json.kind != JSON_CHUNK {
        return Err(Error::MissingGlbJson);
    }
    validate_remaining_chunks(bytes, next)?;
    Ok(json.bytes)
}

fn validate_remaining_chunks(bytes: &[u8], mut offset: usize) -> Result<(), Error> {
    while offset < bytes.len() {
        let (chunk, next) = chunk(bytes, offset)?;
        if chunk.kind == JSON_CHUNK {
            return Err(Error::DuplicateGlbJson);
        }
        offset = next;
    }
    Ok(())
}

struct Chunk<'a> {
    kind: u32,
    bytes: &'a [u8],
}

fn chunk(bytes: &[u8], offset: usize) -> Result<(Chunk<'_>, usize), Error> {
    let header_end = offset
        .checked_add(CHUNK_HEADER_BYTES)
        .ok_or(Error::TruncatedGlb)?;
    if header_end > bytes.len() {
        return Err(Error::TruncatedGlb);
    }
    let length = usize::try_from(read_u32(bytes, offset)?).map_err(|_error| Error::TruncatedGlb)?;
    let kind = read_u32(bytes, offset + 4)?;
    let end = header_end.checked_add(length).ok_or(Error::TruncatedGlb)?;
    let data = bytes.get(header_end..end).ok_or(Error::TruncatedGlb)?;
    Ok((Chunk { kind, bytes: data }, end))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let end = offset.checked_add(4).ok_or(Error::TruncatedGlb)?;
    let raw: [u8; 4] = bytes
        .get(offset..end)
        .ok_or(Error::TruncatedGlb)?
        .try_into()
        .map_err(|_error| Error::TruncatedGlb)?;
    Ok(u32::from_le_bytes(raw))
}

#[cfg(test)]
mod tests {
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
}
