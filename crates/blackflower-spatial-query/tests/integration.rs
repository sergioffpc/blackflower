use blackflower_spatial_query::{Device, EMBREE_VERSION, Triangle, Vec3A};

#[test]
fn linked_version_matches_the_pin() {
    assert_eq!(blackflower_spatial_query::embree_version(), EMBREE_VERSION);
}

#[test]
fn segment_hits_are_bounded_and_ordered() -> Result<(), Box<dyn std::error::Error>> {
    let device = Device::new()?;
    let mut builder = device.create_scene()?;
    let geometry = builder.add_triangles(&[
        Triangle::new([
            Vec3A::new(2.0, -1.0, -1.0),
            Vec3A::new(2.0, 1.0, -1.0),
            Vec3A::new(2.0, 0.0, 1.0),
        ])?,
        Triangle::new([
            Vec3A::new(1.0, -1.0, -1.0),
            Vec3A::new(1.0, 1.0, -1.0),
            Vec3A::new(1.0, 0.0, 1.0),
        ])?,
    ])?;
    let scene = builder.commit()?;

    let mut hits = Vec::with_capacity(2);
    scene.intersect_segment(Vec3A::ZERO, Vec3A::new(3.0, 0.0, 0.0), 2, &mut hits)?;
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].geometry_id(), geometry);
    assert_eq!(hits[0].primitive_id().0, 1);
    assert!((hits[0].distance() - 1.0).abs() < 0.001);
    assert_eq!(hits[1].primitive_id().0, 0);
    assert!((hits[1].distance() - 2.0).abs() < 0.001);
    let closest = scene
        .closest_hit(Vec3A::ZERO, Vec3A::new(3.0, 0.0, 0.0))?
        .ok_or("closest hit was missing")?;
    assert_eq!(closest.primitive_id().0, 1);
    assert!((closest.distance() - 1.0).abs() < 0.001);
    assert!(scene.is_occluded(Vec3A::ZERO, Vec3A::new(3.0, 0.0, 0.0))?);
    assert!(!scene.is_occluded(Vec3A::ZERO, Vec3A::new(0.0, 3.0, 0.0))?);
    Ok(())
}

#[test]
fn empty_scene_and_zero_length_segments_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let device = Device::new()?;
    let scene = device.create_scene()?.commit()?;
    let mut hits = Vec::new();
    scene.intersect_segment(Vec3A::ZERO, Vec3A::ZERO, 8, &mut hits)?;
    assert!(hits.is_empty());
    Ok(())
}
