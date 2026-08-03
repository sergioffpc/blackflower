use crate::{
    AnimationContainer, ClipMarker, ClipMetadata, Error, OzzVersion, SkeletonContainer,
    SkeletonIdentity,
};

const VERSION: OzzVersion = OzzVersion::new(0, 16, 0);
const IDENTITY: SkeletonIdentity = SkeletonIdentity::from_bytes([7; 32]);

#[test]
fn typed_containers_round_trip() -> Result<(), Error> {
    let skeleton = SkeletonContainer::encode(VERSION, IDENTITY, b"skeleton")?;
    assert_eq!(&skeleton[..8], b"BFSKEL\0\0");
    assert_eq!(&skeleton[8..12], &[1, 0, 64, 0]);
    assert_eq!(&skeleton[12..20], &[0, 0, 16, 0, 0, 0, 0, 0]);
    assert_eq!(&skeleton[20..24], &[1, 0, 0, 0]);
    assert_eq!(&skeleton[24..32], &96_u64.to_le_bytes());
    assert_eq!(&skeleton[32..64], &[7; 32]);
    assert_eq!(&skeleton[64..72], &[1, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&skeleton[72..80], &88_u64.to_le_bytes());
    assert_eq!(&skeleton[80..88], &8_u64.to_le_bytes());
    let decoded_skeleton = SkeletonContainer::decode(&skeleton)?;
    assert_eq!(decoded_skeleton.ozz_skeleton(), b"skeleton");
    assert_eq!(decoded_skeleton.identity(), IDENTITY);

    let metadata = ClipMetadata::new("Walk", true, false, [ClipMarker::new("step", 0.5)?])?;
    let animation =
        AnimationContainer::encode(VERSION, IDENTITY, b"animation", &metadata, Some(b"motion"))?;
    let decoded_animation = AnimationContainer::decode(&animation)?;
    assert_eq!(decoded_animation.ozz_animation(), b"animation");
    assert_eq!(decoded_animation.metadata(), &metadata);
    assert_eq!(
        decoded_animation.ozz_root_motion(),
        Some(b"motion".as_slice())
    );
    Ok(())
}

#[test]
fn raw_ozz_wrong_magic_and_schema_are_rejected() -> Result<(), Error> {
    assert!(matches!(
        SkeletonContainer::decode(b"ozz-skeleton"),
        Err(Error::InvalidMagic)
    ));
    let mut skeleton = SkeletonContainer::encode(VERSION, IDENTITY, b"skeleton")?;
    skeleton[0] = b'X';
    assert!(matches!(
        SkeletonContainer::decode(&skeleton),
        Err(Error::InvalidMagic)
    ));
    let mut skeleton = SkeletonContainer::encode(VERSION, IDENTITY, b"skeleton")?;
    skeleton[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(matches!(
        SkeletonContainer::decode(&skeleton),
        Err(Error::UnsupportedSchema(u16::MAX))
    ));
    Ok(())
}

#[test]
fn truncation_and_undeclared_trailing_data_are_rejected() -> Result<(), Error> {
    let mut truncated = SkeletonContainer::encode(VERSION, IDENTITY, b"skeleton")?;
    truncated.pop();
    assert!(matches!(
        SkeletonContainer::decode(&truncated),
        Err(Error::InvalidHeader)
    ));

    let mut trailing = SkeletonContainer::encode(VERSION, IDENTITY, b"skeleton")?;
    trailing.push(0);
    let file_size = u64::try_from(trailing.len()).map_err(|_error| Error::InvalidHeader)?;
    trailing[24..32].copy_from_slice(&file_size.to_le_bytes());
    assert!(matches!(
        SkeletonContainer::decode(&trailing),
        Err(Error::InvalidSection)
    ));
    Ok(())
}

#[test]
fn flags_unknown_sections_offsets_and_padding_are_rejected() -> Result<(), Error> {
    let mut header_flags = SkeletonContainer::encode(VERSION, IDENTITY, b"skeleton")?;
    header_flags[18..20].copy_from_slice(&1_u16.to_le_bytes());
    assert!(matches!(
        SkeletonContainer::decode(&header_flags),
        Err(Error::InvalidHeader)
    ));

    let mut section_flags = SkeletonContainer::encode(VERSION, IDENTITY, b"skeleton")?;
    section_flags[68..72].copy_from_slice(&1_u32.to_le_bytes());
    assert!(matches!(
        SkeletonContainer::decode(&section_flags),
        Err(Error::InvalidSection)
    ));

    let mut unknown_section = SkeletonContainer::encode(VERSION, IDENTITY, b"skeleton")?;
    unknown_section[64..68].copy_from_slice(&99_u32.to_le_bytes());
    assert!(matches!(
        SkeletonContainer::decode(&unknown_section),
        Err(Error::InvalidSection)
    ));

    let mut unaligned = SkeletonContainer::encode(VERSION, IDENTITY, b"skeleton")?;
    unaligned[72..80].copy_from_slice(&89_u64.to_le_bytes());
    assert!(matches!(
        SkeletonContainer::decode(&unaligned),
        Err(Error::InvalidSection)
    ));

    let metadata = ClipMetadata::new("Clip", false, false, [])?;
    let mut animation = AnimationContainer::encode(VERSION, IDENTITY, b"a", &metadata, None)?;
    let first_descriptor = 64;
    let first_offset = usize::try_from(u64::from_le_bytes(
        <[u8; 8]>::try_from(&animation[first_descriptor + 8..first_descriptor + 16])
            .map_err(|_error| Error::InvalidHeader)?,
    ))
    .map_err(|_error| Error::InvalidHeader)?;
    animation[first_offset + 1] = 1;
    assert!(matches!(
        AnimationContainer::decode(&animation),
        Err(Error::InvalidSection)
    ));

    let mut overlap = AnimationContainer::encode(VERSION, IDENTITY, b"animation", &metadata, None)?;
    let first_offset = u64::from_le_bytes(
        <[u8; 8]>::try_from(&overlap[72..80]).map_err(|_error| Error::InvalidHeader)?,
    );
    overlap[96..104].copy_from_slice(&first_offset.to_le_bytes());
    assert!(matches!(
        AnimationContainer::decode(&overlap),
        Err(Error::InvalidSection)
    ));
    Ok(())
}
