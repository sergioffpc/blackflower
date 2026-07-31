use super::{inspect_animation_ozz, inspect_skeleton_ozz};
use crate::Error;

const SKELETON: &[u8] = include_bytes!("../../vendor/ozz-animation/media/bin/baked_skeleton.ozz");
const ANIMATION: &[u8] = include_bytes!("../../vendor/ozz-animation/media/bin/baked_animation.ozz");
const OTHER_SKELETON: &[u8] =
    include_bytes!("../../vendor/ozz-animation/media/bin/robot_skeleton.ozz");

#[test]
fn vendored_payloads_can_be_inspected_for_cooking() -> Result<(), Error> {
    let skeleton = inspect_skeleton_ozz(SKELETON)?;
    let animation = inspect_animation_ozz(ANIMATION)?;
    assert!(skeleton.joint_count > 1);
    assert_eq!(animation.track_count, skeleton.joint_count);
    assert_eq!(
        skeleton.identity.as_bytes(),
        &[
            0x17, 0xb4, 0x9c, 0x4b, 0xf2, 0x33, 0x22, 0x2e, 0x60, 0x03, 0x58, 0x40, 0x5b, 0xce,
            0xe1, 0xd5, 0x26, 0xd8, 0x40, 0xee, 0x0f, 0x79, 0x56, 0xa6, 0x81, 0x20, 0x92, 0xcf,
            0x93, 0xea, 0x06, 0xfb,
        ]
    );
    assert_eq!(
        inspect_skeleton_ozz(OTHER_SKELETON)?.identity.as_bytes(),
        &[
            0x57, 0x44, 0x12, 0x1f, 0xb4, 0xf8, 0x98, 0x85, 0xc6, 0xdb, 0x77, 0xa1, 0x55, 0xfb,
            0x24, 0x1b, 0xcc, 0xa2, 0xde, 0xac, 0xd6, 0xe3, 0xa0, 0xb9, 0x44, 0x1f, 0xc1, 0xaa,
            0x03, 0xfb, 0x36, 0xcb,
        ]
    );
    Ok(())
}
