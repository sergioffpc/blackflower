use blackflower_physics::{
    BodySettings, CharacterSettings, CompoundShapeChild, ContactEventKind, Error, GroundState,
    MAX_CONVEX_HULL_POINTS, MotionType, Shape, World, jolt_version,
};
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
    world.optimize_broad_phase()?;

    let initial_position = world.position(sphere)?;
    world.step(NonZeroU32::MIN)?;
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
        CharacterSettings::new(Shape::sphere(0.5)?),
        Err(Error::InvalidCharacterShape),
    );
    assert_eq!(
        Shape::convex_hull(vec![Vec3A::ZERO; 3]),
        Err(Error::InvalidShape),
    );
    assert_eq!(
        Shape::convex_hull(vec![Vec3A::ZERO; MAX_CONVEX_HULL_POINTS + 1]),
        Err(Error::InvalidShape),
    );
    assert_eq!(
        Shape::triangle_mesh([Vec3A::ZERO, Vec3A::X, Vec3A::Z], [[0, 1, 3]],),
        Err(Error::InvalidShape),
    );
    assert_eq!(
        Shape::compound(Vec::<CompoundShapeChild>::new()),
        Err(Error::InvalidShape),
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

    world.create_body(settings.clone())?;
    assert_eq!(
        world.create_body(settings),
        Err(Error::BodyCapacityExhausted),
    );
    Ok(())
}

#[test]
fn convex_hulls_support_dynamic_bodies_and_rays() -> Result<(), Error> {
    let mut world = World::new()?;
    let body = world.create_body(
        BodySettings::new(cube_hull(0.5)?, MotionType::Dynamic)
            .with_position(Vec3A::new(0.0, 2.0, 0.0))?,
    )?;

    let hit = world
        .cast_ray(Vec3A::new(0.0, 2.0, -2.0), Vec3A::new(0.0, 0.0, 4.0))?
        .ok_or(Error::NativeContract)?;
    assert_eq!(hit.body, body);

    world.step(NonZeroU32::MIN)?;
    assert!(world.position(body)?.y < 2.0);
    Ok(())
}

#[test]
fn compounds_apply_child_transforms() -> Result<(), Error> {
    let children = vec![
        CompoundShapeChild::new(cube_hull(0.5)?).with_position(Vec3A::new(-1.0, 0.0, 0.0))?,
        CompoundShapeChild::new(cube_hull(0.5)?).with_position(Vec3A::new(1.0, 0.0, 0.0))?,
    ];
    let mut world = World::new()?;
    let body = world.create_body(BodySettings::new(
        Shape::compound(children)?,
        MotionType::Static,
    ))?;

    let hit = world
        .cast_ray(Vec3A::new(1.0, 0.0, -2.0), Vec3A::new(0.0, 0.0, 4.0))?
        .ok_or(Error::NativeContract)?;
    assert_eq!(hit.body, body);
    assert!((hit.position.z + 0.5).abs() < 0.001);
    Ok(())
}

#[test]
fn triangle_meshes_are_static_and_collidable() -> Result<(), Error> {
    let mesh = floor_mesh()?;
    let mut world = World::new()?;
    assert_eq!(
        world.create_body(BodySettings::new(mesh.clone(), MotionType::Dynamic)),
        Err(Error::StaticShapeRequiresStaticBody),
    );
    let body = world.create_body(BodySettings::new(mesh, MotionType::Static))?;

    let hit = world
        .cast_ray(Vec3A::new(0.0, 2.0, 0.0), Vec3A::new(0.0, -4.0, 0.0))?
        .ok_or(Error::NativeContract)?;
    assert_eq!(hit.body, body);
    assert!(hit.position.abs_diff_eq(Vec3A::ZERO, 0.001));
    assert!(hit.normal.abs_diff_eq(Vec3A::Y, 0.001));
    Ok(())
}

#[test]
fn degenerate_convex_hulls_report_shape_creation_failure() -> Result<(), Error> {
    let hull = Shape::convex_hull([Vec3A::ZERO; 4])?;
    let mut world = World::new()?;

    assert_eq!(
        world.create_body(BodySettings::new(hull, MotionType::Dynamic)),
        Err(Error::ShapeCreationFailed),
    );
    Ok(())
}

fn cube_hull(half_extent: f32) -> Result<Shape, Error> {
    let points = [-half_extent, half_extent]
        .into_iter()
        .flat_map(|x| {
            [-half_extent, half_extent].into_iter().flat_map(move |y| {
                [-half_extent, half_extent]
                    .into_iter()
                    .map(move |z| Vec3A::new(x, y, z))
            })
        })
        .collect::<Vec<_>>();
    Shape::convex_hull(points)
}

fn floor_mesh() -> Result<Shape, Error> {
    Shape::triangle_mesh(
        [
            Vec3A::new(-2.0, 0.0, -2.0),
            Vec3A::new(-2.0, 0.0, 2.0),
            Vec3A::new(2.0, 0.0, 2.0),
            Vec3A::new(2.0, 0.0, -2.0),
        ],
        [[0, 1, 2], [0, 2, 3]],
    )
}

#[test]
fn bodies_expose_rotation_angular_motion_and_force_commands() -> Result<(), Error> {
    let mut world = World::new()?;
    let body = world.create_body(BodySettings::new(
        Shape::cuboid(Vec3A::splat(0.5))?,
        MotionType::Dynamic,
    ))?;
    let rotation = Quat::from_rotation_y(0.5);

    world.set_rotation(body, rotation)?;
    world.set_angular_velocity(body, Vec3A::new(0.0, 1.0, 0.0))?;
    world.add_force(body, Vec3A::new(2.0, 0.0, 0.0))?;
    world.add_force_at_point(body, Vec3A::new(0.0, 0.0, 1.0), Vec3A::X)?;
    world.add_torque(body, Vec3A::new(0.0, 1.0, 0.0))?;
    world.add_impulse(body, Vec3A::new(1.0, 0.0, 0.0))?;
    world.add_impulse_at_point(body, Vec3A::Y, Vec3A::X)?;
    world.add_angular_impulse(body, Vec3A::new(0.0, 0.5, 0.0))?;

    assert!(world.rotation(body)?.dot(rotation).abs() > 0.999);
    assert!(world.angular_velocity(body)?.length() > 0.0);
    assert!(world.linear_velocity(body)?.length() > 0.0);
    Ok(())
}

#[test]
fn contact_events_capture_manifold_geometry() -> Result<(), Error> {
    let mut world = World::new()?;
    let floor = world.create_body(
        BodySettings::new(
            Shape::cuboid(Vec3A::new(2.0, 0.5, 2.0))?,
            MotionType::Static,
        )
        .with_position(Vec3A::new(0.0, -0.5, 0.0))?,
    )?;
    let sphere = world.create_body(
        BodySettings::new(Shape::sphere(0.5)?, MotionType::Dynamic)
            .with_position(Vec3A::new(0.0, 0.4, 0.0))?,
    )?;

    world.step(NonZeroU32::MIN)?;
    let contacts = world.contact_events()?;
    let contact = contacts
        .iter()
        .find(|contact| contact.body1 == floor && contact.body2 == sphere)
        .ok_or(Error::NativeContract)?;
    let manifold = contact.manifold.as_ref().ok_or(Error::NativeContract)?;

    assert_eq!(contact.kind, ContactEventKind::Added);
    assert!(manifold.normal.is_finite());
    assert!(!manifold.points.is_empty());

    world.destroy_body(sphere)?;
    world.step(NonZeroU32::MIN)?;
    let removed = world
        .contact_events()?
        .into_iter()
        .find(|contact| contact.body1 == floor && contact.body2 == sphere)
        .ok_or(Error::NativeContract)?;
    assert_eq!(removed.kind, ContactEventKind::Removed);
    assert!(removed.manifold.is_none());
    Ok(())
}

#[test]
fn ray_casts_capture_closest_visibility_hits() -> Result<(), Error> {
    let mut world = World::new()?;
    let near = world.create_body(
        BodySettings::new(Shape::sphere(0.5)?, MotionType::Static)
            .with_position(Vec3A::new(0.0, 0.0, 2.0))?,
    )?;
    world.create_body(
        BodySettings::new(Shape::sphere(0.5)?, MotionType::Static)
            .with_position(Vec3A::new(0.0, 0.0, 4.0))?,
    )?;

    let hit = world
        .cast_ray(Vec3A::ZERO, Vec3A::new(0.0, 0.0, 10.0))?
        .ok_or(Error::NativeContract)?;
    assert_eq!(hit.body, near);
    assert!((hit.fraction - 0.15).abs() < 0.001);
    assert!(hit.position.abs_diff_eq(Vec3A::new(0.0, 0.0, 1.5), 0.001));
    assert!(hit.normal.abs_diff_eq(Vec3A::NEG_Z, 0.001));
    assert!(
        world
            .cast_ray(Vec3A::ZERO, Vec3A::new(10.0, 0.0, 0.0))?
            .is_none()
    );
    assert_eq!(
        world.cast_ray(Vec3A::ZERO, Vec3A::ZERO),
        Err(Error::InvalidRay)
    );
    Ok(())
}

#[test]
fn rigid_body_character_reports_ground_state_and_owns_its_body() -> Result<(), Error> {
    let mut world = World::new()?;
    world.create_body(
        BodySettings::new(
            Shape::cuboid(Vec3A::new(10.0, 0.5, 10.0))?,
            MotionType::Static,
        )
        .with_position(Vec3A::new(0.0, -0.5, 0.0))?,
    )?;
    let character = world.create_character(
        CharacterSettings::new(Shape::capsule(0.5, 0.5)?)?
            .with_position(Vec3A::new(0.0, 2.0, 0.0))?,
    )?;
    world.set_character_linear_velocity(character, Vec3A::new(1.0, 0.0, 0.0))?;
    for _step in 0..480 {
        world.step(NonZeroU32::MIN)?;
        world.refresh_character_ground_state(character, 0.05)?;
    }
    let state = world.character_state(character)?;

    assert_eq!(state.body, character.body());
    assert_eq!(state.ground.state, GroundState::OnGround);
    assert!(state.ground.body.is_some());
    assert_eq!(
        world.destroy_body(character.body()),
        Err(Error::BodyOwnedByCharacter)
    );
    assert!(world.is_character_alive(character)?);
    world.destroy_character(character)?;
    assert!(!world.is_character_alive(character)?);
    assert_eq!(
        world.character_state(character),
        Err(Error::CharacterNotFound)
    );
    Ok(())
}
