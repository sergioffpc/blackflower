use super::{RestTransform, RigJoint, SkeletonIdentity};
use crate::Error;

fn root(rotation: [f32; 4]) -> RigJoint<'static> {
    RigJoint {
        name: "root",
        parent: None,
        rest: RestTransform {
            translation: [-0.0, 1.0, 2.0],
            rotation,
            scale: [1.0; 3],
        },
    }
}

#[test]
fn identity_canonicalizes_zero_and_quaternion_sign() -> Result<(), Error> {
    let positive = SkeletonIdentity::from_rig(&[root([0.0, 0.0, 0.0, 1.0])])?;
    let negative = SkeletonIdentity::from_rig(&[root([-0.0, -0.0, -0.0, -1.0])])?;
    assert_eq!(positive, negative);
    Ok(())
}

#[test]
fn identity_changes_with_rest_pose() -> Result<(), Error> {
    let first = SkeletonIdentity::from_rig(&[root([0.0, 0.0, 0.0, 1.0])])?;
    let mut changed = root([0.0, 0.0, 0.0, 1.0]);
    changed.rest.translation[0] = 0.5;
    let second = SkeletonIdentity::from_rig(&[changed])?;
    assert_ne!(first, second);
    Ok(())
}

#[test]
fn invalid_parent_is_rejected() {
    let mut joint = root([0.0, 0.0, 0.0, 1.0]);
    joint.parent = Some(0);
    assert_eq!(SkeletonIdentity::from_rig(&[joint]), Err(Error::InvalidRig));
}
