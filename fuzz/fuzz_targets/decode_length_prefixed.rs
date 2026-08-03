#![no_main]

use blackflower_animation_format::AnimationContainer;
use blackflower_networking_replication::SnapshotDelta;
use blackflower_rendering_models::ModelAsset;
use libfuzzer_sys::fuzz_target;

const HEADER_SIZE: usize = 64;
const DESCRIPTOR_SIZE: usize = 24;
const OZZ_ANIMATION_OFFSET: usize = HEADER_SIZE + 2 * DESCRIPTOR_SIZE;
const METADATA_OFFSET: usize = 120;

fuzz_target!(|bytes: &[u8]| {
    let _delta = SnapshotDelta::decode(bytes);
    let _model = ModelAsset::from_bytes(bytes.to_vec().into());
    if let Some(animation) = animation_with_metadata(bytes) {
        let _animation = AnimationContainer::decode(&animation);
    }
});

fn animation_with_metadata(metadata: &[u8]) -> Option<Vec<u8>> {
    let metadata_size = metadata.len().max(1);
    let file_size = METADATA_OFFSET.checked_add(metadata_size)?;
    let mut bytes = vec![0_u8; file_size];

    bytes[..8].copy_from_slice(b"BFANIM\0\0");
    write_u16(&mut bytes, 8, 1);
    write_u16(&mut bytes, 10, HEADER_SIZE as u16);
    write_u32(&mut bytes, 20, 2);
    write_u64(&mut bytes, 24, u64::try_from(file_size).ok()?);

    write_u32(&mut bytes, HEADER_SIZE, 2);
    write_u64(
        &mut bytes,
        HEADER_SIZE + 8,
        u64::try_from(OZZ_ANIMATION_OFFSET).ok()?,
    );
    write_u64(&mut bytes, HEADER_SIZE + 16, 1);
    bytes[OZZ_ANIMATION_OFFSET] = 1;

    let metadata_descriptor = HEADER_SIZE + DESCRIPTOR_SIZE;
    write_u32(&mut bytes, metadata_descriptor, 3);
    write_u64(
        &mut bytes,
        metadata_descriptor + 8,
        u64::try_from(METADATA_OFFSET).ok()?,
    );
    write_u64(
        &mut bytes,
        metadata_descriptor + 16,
        u64::try_from(metadata_size).ok()?,
    );
    if !metadata.is_empty() {
        bytes[METADATA_OFFSET..].copy_from_slice(metadata);
    }
    Some(bytes)
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
