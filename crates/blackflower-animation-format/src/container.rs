use crate::{ClipMetadata, Error, SkeletonIdentity};

/// Current `.bfskel` and `.bfanim` container schema.
pub const CONTAINER_SCHEMA: u16 = 1;
/// Fixed Blackflower animation container header size.
pub const HEADER_SIZE: usize = 64;
/// `.bfskel` magic.
pub const SKELETON_MAGIC: [u8; 8] = *b"BFSKEL\0\0";
/// `.bfanim` magic.
pub const ANIMATION_MAGIC: [u8; 8] = *b"BFANIM\0\0";

const DESCRIPTOR_SIZE: usize = 24;
const SECTION_ALIGNMENT: usize = 8;
const OZZ_SKELETON: u32 = 1;
const OZZ_ANIMATION: u32 = 2;
const CLIP_METADATA: u32 = 3;
const OZZ_ROOT_MOTION: u32 = 4;

/// ozz-animation version required to decode private payload sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OzzVersion {
    /// Major version.
    pub major: u16,
    /// Minor version.
    pub minor: u16,
    /// Patch version.
    pub patch: u16,
}

impl OzzVersion {
    /// Construct a version triplet.
    #[must_use]
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

/// Validated borrowed `.bfskel` contents.
#[derive(Debug, Clone, Copy)]
pub struct SkeletonContainer<'a> {
    version: OzzVersion,
    identity: SkeletonIdentity,
    ozz_skeleton: &'a [u8],
}

impl<'a> SkeletonContainer<'a> {
    /// Validate and borrow one complete `.bfskel`.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        let parsed = parse(bytes, SKELETON_MAGIC, &[OZZ_SKELETON])?;
        Ok(Self {
            version: parsed.version,
            identity: parsed.identity,
            ozz_skeleton: parsed.sections[0],
        })
    }

    /// Encode one deterministic `.bfskel`.
    pub fn encode(
        version: OzzVersion,
        identity: SkeletonIdentity,
        ozz_skeleton: &[u8],
    ) -> Result<Vec<u8>, Error> {
        encode(
            SKELETON_MAGIC,
            version,
            identity,
            &[(OZZ_SKELETON, ozz_skeleton)],
        )
    }

    /// Required ozz version.
    #[must_use]
    pub const fn ozz_version(&self) -> OzzVersion {
        self.version
    }

    /// Full rig identity.
    #[must_use]
    pub const fn identity(&self) -> SkeletonIdentity {
        self.identity
    }

    /// Private ozz skeleton payload.
    #[must_use]
    pub const fn ozz_skeleton(&self) -> &'a [u8] {
        self.ozz_skeleton
    }
}

/// Validated borrowed `.bfanim` contents.
#[derive(Debug, Clone)]
pub struct AnimationContainer<'a> {
    version: OzzVersion,
    identity: SkeletonIdentity,
    ozz_animation: &'a [u8],
    metadata: ClipMetadata,
    ozz_root_motion: Option<&'a [u8]>,
}

impl<'a> AnimationContainer<'a> {
    /// Validate and borrow one complete `.bfanim`.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        let section_count = section_count(bytes, ANIMATION_MAGIC)?;
        let expected = match section_count {
            2 => &[OZZ_ANIMATION, CLIP_METADATA][..],
            3 => &[OZZ_ANIMATION, CLIP_METADATA, OZZ_ROOT_MOTION][..],
            _ => return Err(Error::InvalidSection),
        };
        let parsed = parse(bytes, ANIMATION_MAGIC, expected)?;
        Ok(Self {
            version: parsed.version,
            identity: parsed.identity,
            ozz_animation: parsed.sections[0],
            metadata: ClipMetadata::decode(parsed.sections[1])?,
            ozz_root_motion: parsed.sections.get(2).copied(),
        })
    }

    /// Encode one deterministic `.bfanim`.
    pub fn encode(
        version: OzzVersion,
        identity: SkeletonIdentity,
        ozz_animation: &[u8],
        metadata: &ClipMetadata,
        ozz_root_motion: Option<&[u8]>,
    ) -> Result<Vec<u8>, Error> {
        let encoded_metadata = metadata.encode()?;
        let mut sections = vec![
            (OZZ_ANIMATION, ozz_animation),
            (CLIP_METADATA, encoded_metadata.as_slice()),
        ];
        if let Some(root_motion) = ozz_root_motion {
            sections.push((OZZ_ROOT_MOTION, root_motion));
        }
        encode(ANIMATION_MAGIC, version, identity, &sections)
    }

    /// Required ozz version.
    #[must_use]
    pub const fn ozz_version(&self) -> OzzVersion {
        self.version
    }

    /// Required skeleton identity.
    #[must_use]
    pub const fn skeleton_identity(&self) -> SkeletonIdentity {
        self.identity
    }

    /// Private ozz animation payload.
    #[must_use]
    pub const fn ozz_animation(&self) -> &'a [u8] {
        self.ozz_animation
    }

    /// Typed clip metadata.
    #[must_use]
    pub const fn metadata(&self) -> &ClipMetadata {
        &self.metadata
    }

    /// Optional private ozz root-motion payload.
    #[must_use]
    pub const fn ozz_root_motion(&self) -> Option<&'a [u8]> {
        self.ozz_root_motion
    }
}

struct Parsed<'a> {
    version: OzzVersion,
    identity: SkeletonIdentity,
    sections: Vec<&'a [u8]>,
}

fn parse<'a>(
    bytes: &'a [u8],
    expected_magic: [u8; 8],
    expected_kinds: &[u32],
) -> Result<Parsed<'a>, Error> {
    validate_header(bytes, expected_magic, expected_kinds.len())?;
    let version = OzzVersion {
        major: read_u16(bytes, 12)?,
        minor: read_u16(bytes, 14)?,
        patch: read_u16(bytes, 16)?,
    };
    let identity_bytes = bytes.get(32..64).ok_or(Error::InvalidHeader)?;
    let identity = SkeletonIdentity::from_bytes(
        <[u8; 32]>::try_from(identity_bytes).map_err(|_error| Error::InvalidHeader)?,
    );
    let sections = parse_sections(bytes, expected_kinds)?;
    Ok(Parsed {
        version,
        identity,
        sections,
    })
}

fn parse_sections<'a>(bytes: &'a [u8], expected_kinds: &[u32]) -> Result<Vec<&'a [u8]>, Error> {
    let table_end = HEADER_SIZE
        .checked_add(
            DESCRIPTOR_SIZE
                .checked_mul(expected_kinds.len())
                .ok_or(Error::InvalidHeader)?,
        )
        .ok_or(Error::InvalidHeader)?;
    let mut expected_offset = align(table_end, SECTION_ALIGNMENT)?;
    let mut sections = Vec::with_capacity(expected_kinds.len());
    for (index, expected_kind) in expected_kinds.iter().copied().enumerate() {
        let last = index + 1 == expected_kinds.len();
        let (section, next_offset) =
            parse_section(bytes, index, expected_kind, expected_offset, last)?;
        sections.push(section);
        expected_offset = next_offset;
    }
    if expected_offset != bytes.len() {
        return Err(Error::InvalidSection);
    }
    Ok(sections)
}

fn parse_section(
    bytes: &[u8],
    index: usize,
    expected_kind: u32,
    expected_offset: usize,
    last: bool,
) -> Result<(&[u8], usize), Error> {
    let descriptor = HEADER_SIZE
        .checked_add(
            index
                .checked_mul(DESCRIPTOR_SIZE)
                .ok_or(Error::InvalidSection)?,
        )
        .ok_or(Error::InvalidSection)?;
    if read_u32(bytes, descriptor)? != expected_kind || read_u32(bytes, descriptor + 4)? != 0 {
        return Err(Error::InvalidSection);
    }
    let offset = read_usize_u64(bytes, descriptor + 8)?;
    let length = read_usize_u64(bytes, descriptor + 16)?;
    if length == 0 || offset != expected_offset || !offset.is_multiple_of(SECTION_ALIGNMENT) {
        return Err(Error::InvalidSection);
    }
    let end = offset.checked_add(length).ok_or(Error::InvalidSection)?;
    let section = bytes.get(offset..end).ok_or(Error::InvalidSection)?;
    if last {
        return Ok((section, end));
    }
    let aligned = align(end, SECTION_ALIGNMENT)?;
    if bytes
        .get(end..aligned)
        .ok_or(Error::InvalidSection)?
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(Error::InvalidSection);
    }
    Ok((section, aligned))
}

fn validate_header(
    bytes: &[u8],
    expected_magic: [u8; 8],
    expected_sections: usize,
) -> Result<(), Error> {
    if bytes.get(..8) != Some(expected_magic.as_slice()) {
        return Err(Error::InvalidMagic);
    }
    let schema = read_u16(bytes, 8)?;
    if schema != CONTAINER_SCHEMA {
        return Err(Error::UnsupportedSchema(schema));
    }
    let header_size = usize::from(read_u16(bytes, 10)?);
    let section_count = read_usize_u32(bytes, 20)?;
    let file_size = read_usize_u64(bytes, 24)?;
    if header_size != HEADER_SIZE
        || read_u16(bytes, 18)? != 0
        || section_count != expected_sections
        || file_size != bytes.len()
    {
        return Err(Error::InvalidHeader);
    }
    Ok(())
}

fn section_count(bytes: &[u8], expected_magic: [u8; 8]) -> Result<usize, Error> {
    if bytes.get(..8) != Some(expected_magic.as_slice()) {
        return Err(Error::InvalidMagic);
    }
    read_usize_u32(bytes, 20)
}

fn encode(
    magic: [u8; 8],
    version: OzzVersion,
    identity: SkeletonIdentity,
    sections: &[(u32, &[u8])],
) -> Result<Vec<u8>, Error> {
    let section_count = u32::try_from(sections.len()).map_err(|_error| Error::AssetTooLarge)?;
    let table_size = DESCRIPTOR_SIZE
        .checked_mul(sections.len())
        .ok_or(Error::AssetTooLarge)?;
    let initial_size = HEADER_SIZE
        .checked_add(table_size)
        .ok_or(Error::AssetTooLarge)?;
    let mut output = vec![0_u8; align(initial_size, SECTION_ALIGNMENT)?];
    output[..8].copy_from_slice(&magic);
    write_u16(&mut output, 8, CONTAINER_SCHEMA)?;
    write_u16(
        &mut output,
        10,
        u16::try_from(HEADER_SIZE).map_err(|_error| Error::AssetTooLarge)?,
    )?;
    write_u16(&mut output, 12, version.major)?;
    write_u16(&mut output, 14, version.minor)?;
    write_u16(&mut output, 16, version.patch)?;
    write_u32(&mut output, 20, section_count)?;
    output[32..64].copy_from_slice(identity.as_bytes());

    for (index, (kind, payload)) in sections.iter().enumerate() {
        if payload.is_empty() {
            return Err(Error::InvalidSection);
        }
        let descriptor = HEADER_SIZE
            .checked_add(
                index
                    .checked_mul(DESCRIPTOR_SIZE)
                    .ok_or(Error::AssetTooLarge)?,
            )
            .ok_or(Error::AssetTooLarge)?;
        write_u32(&mut output, descriptor, *kind)?;
        let output_offset = u64::try_from(output.len()).map_err(|_error| Error::AssetTooLarge)?;
        write_u64(&mut output, descriptor + 8, output_offset)?;
        write_u64(
            &mut output,
            descriptor + 16,
            u64::try_from(payload.len()).map_err(|_error| Error::AssetTooLarge)?,
        )?;
        output.extend_from_slice(payload);
        if index + 1 != sections.len() {
            let aligned = align(output.len(), SECTION_ALIGNMENT)?;
            output.resize(aligned, 0);
        }
    }
    let file_size = u64::try_from(output.len()).map_err(|_error| Error::AssetTooLarge)?;
    write_u64(&mut output, 24, file_size)?;
    Ok(output)
}

fn align(value: usize, alignment: usize) -> Result<usize, Error> {
    value
        .checked_add(alignment - 1)
        .map(|sum| sum & !(alignment - 1))
        .ok_or(Error::AssetTooLarge)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    let raw = bytes.get(offset..offset + 2).ok_or(Error::InvalidHeader)?;
    Ok(u16::from_le_bytes(
        <[u8; 2]>::try_from(raw).map_err(|_error| Error::InvalidHeader)?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let raw = bytes.get(offset..offset + 4).ok_or(Error::InvalidHeader)?;
    Ok(u32::from_le_bytes(
        <[u8; 4]>::try_from(raw).map_err(|_error| Error::InvalidHeader)?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    let raw = bytes.get(offset..offset + 8).ok_or(Error::InvalidHeader)?;
    Ok(u64::from_le_bytes(
        <[u8; 8]>::try_from(raw).map_err(|_error| Error::InvalidHeader)?,
    ))
}

fn read_usize_u32(bytes: &[u8], offset: usize) -> Result<usize, Error> {
    usize::try_from(read_u32(bytes, offset)?).map_err(|_error| Error::InvalidHeader)
}

fn read_usize_u64(bytes: &[u8], offset: usize) -> Result<usize, Error> {
    usize::try_from(read_u64(bytes, offset)?).map_err(|_error| Error::InvalidHeader)
}

fn write_u16(output: &mut [u8], offset: usize, value: u16) -> Result<(), Error> {
    output
        .get_mut(offset..offset + 2)
        .ok_or(Error::AssetTooLarge)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u32(output: &mut [u8], offset: usize, value: u32) -> Result<(), Error> {
    output
        .get_mut(offset..offset + 4)
        .ok_or(Error::AssetTooLarge)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u64(output: &mut [u8], offset: usize, value: u64) -> Result<(), Error> {
    output
        .get_mut(offset..offset + 8)
        .ok_or(Error::AssetTooLarge)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/container.rs"]
mod tests;
