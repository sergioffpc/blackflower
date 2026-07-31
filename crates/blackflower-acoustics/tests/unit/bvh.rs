use super::*;

#[test]
fn bvh_build_and_hits_are_stable() -> Result<(), Error> {
    let triangles = vec![QuantizedTriangle::new(
        [
            PositionMm::new(1_000, -1_000, -1_000),
            PositionMm::new(1_000, 1_000, -1_000),
            PositionMm::new(1_000, 0, 1_000),
        ],
        2,
    )?];
    let first = AcousticBvh::build(&triangles)?;
    let second = AcousticBvh::build(&triangles)?;
    assert_eq!(first, second);
    let mut hits = Vec::new();
    first.intersect_segment(
        &triangles,
        PositionMm::new(0, 0, 0),
        PositionMm::new(2_000, 0, 0),
        8,
        &mut hits,
    );
    assert_eq!(hits[0].distance_mm, 1_000);
    assert_eq!(hits[0].material_index, 2);
    Ok(())
}
