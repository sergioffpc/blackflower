use super::{RestTransform, RigJoint, SkeletonIdentity};
use crate::Error;
use glam::{Quat, Vec3};

fn root(rotation: Quat) -> RigJoint<'static> {
    RigJoint {
        name: "root",
        parent: None,
        rest: RestTransform {
            translation: Vec3::new(-0.0, 1.0, 2.0),
            rotation,
            scale: Vec3::ONE,
        },
    }
}

#[test]
fn identity_canonicalizes_zero_and_quaternion_sign() -> Result<(), Error> {
    let positive = SkeletonIdentity::from_rig(&[root(Quat::IDENTITY)])?;
    let negative = SkeletonIdentity::from_rig(&[root(-Quat::IDENTITY)])?;
    assert_eq!(positive, negative);
    Ok(())
}

#[test]
fn identity_changes_with_rest_pose() -> Result<(), Error> {
    let first = SkeletonIdentity::from_rig(&[root(Quat::IDENTITY)])?;
    let mut changed = root(Quat::IDENTITY);
    changed.rest.translation.x = 0.5;
    let second = SkeletonIdentity::from_rig(&[changed])?;
    assert_ne!(first, second);
    Ok(())
}

#[test]
fn invalid_parent_is_rejected() {
    let mut joint = root(Quat::IDENTITY);
    joint.parent = Some(0);
    assert_eq!(SkeletonIdentity::from_rig(&[joint]), Err(Error::InvalidRig));
}
