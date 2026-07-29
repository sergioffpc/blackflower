use blackflower_physics::{BodySettings, Error, MotionType, Shape, StepDelta, World, jolt_version};
use glam::{Quat, Vec3A};
use std::num::NonZeroU32;

#[test]
fn bindings_report_the_pinned_jolt_version() {
    assert_eq!(jolt_version(), (5, 6, 0));
}

#[test]
fn world_steps_a_dynamic_sphere() -> Result<(), Error> {
    let mut world = World::new()?;
    let floor = BodySettings::new(
        Shape::cuboid(Vec3A::new(100.0, 1.0, 100.0))?,
        MotionType::Static,
    )
    .with_position(Vec3A::new(0.0, -1.0, 0.0))?;
    world.create_body(floor)?;

    let sphere = BodySettings::new(Shape::sphere(0.5)?, MotionType::Dynamic)
        .with_position(Vec3A::new(0.0, 2.0, 0.0))?;
    let sphere = world.create_body(sphere)?;
    world.set_linear_velocity(sphere, Vec3A::new(0.0, -5.0, 0.0))?;

    let initial_position = world.position(sphere)?;
    world.step(StepDelta::from_seconds(1.0 / 60.0)?, NonZeroU32::MIN)?;
    let stepped_position = world.position(sphere)?;

    assert!(world.is_alive(sphere)?);
    assert!(stepped_position.y < initial_position.y);
    Ok(())
}

#[test]
fn body_handles_are_world_scoped_and_detect_destruction() -> Result<(), Error> {
    let mut first = World::new()?;
    let second = World::new()?;
    let body = first.create_body(BodySettings::new(Shape::sphere(0.5)?, MotionType::Dynamic))?;

    assert_eq!(second.is_alive(body), Err(Error::WrongWorld));
    first.destroy_body(body)?;
    assert!(!first.is_alive(body)?);
    assert_eq!(first.position(body), Err(Error::BodyNotFound));
    Ok(())
}

#[test]
fn safe_values_reject_invalid_native_inputs() -> Result<(), Error> {
    assert_eq!(Shape::sphere(0.0), Err(Error::InvalidShape));
    assert_eq!(
        Shape::cuboid(Vec3A::new(1.0, f32::NAN, 1.0)),
        Err(Error::InvalidShape),
    );
    assert_eq!(
        settings_with_rotation(Quat::from_xyzw(0.0, 0.0, 0.0, 2.0)),
        Err(Error::InvalidRotation),
    );
    assert_eq!(
        StepDelta::from_seconds(f32::INFINITY),
        Err(Error::InvalidStepDelta),
    );

    let settings = BodySettings::new(Shape::sphere(0.5)?, MotionType::Dynamic);
    assert_eq!(
        settings.with_position(Vec3A::new(f32::NAN, 0.0, 0.0)),
        Err(Error::InvalidVector),
    );
    Ok(())
}

#[test]
fn public_vector_uses_glam_simd_storage() {
    assert_eq!(std::mem::size_of::<Vec3A>(), 16);
    assert_eq!(std::mem::align_of::<Vec3A>(), 16);
}

fn settings_with_rotation(rotation: Quat) -> Result<BodySettings, Error> {
    BodySettings::new(Shape::sphere(0.5)?, MotionType::Dynamic).with_rotation(rotation)
}

#[test]
fn configured_body_capacity_is_enforced() -> Result<(), Error> {
    let mut world = World::builder().max_bodies(NonZeroU32::MIN).build()?;
    let settings = BodySettings::new(Shape::sphere(0.5)?, MotionType::Dynamic);

    world.create_body(settings)?;
    assert_eq!(
        world.create_body(settings),
        Err(Error::BodyCapacityExhausted),
    );
    Ok(())
}
