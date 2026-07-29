use std::num::NonZeroU32;

use blackflower_navigation::{
    Error, NavMesh, NavMeshParams, PathPointKind, QueryFilter, recastnavigation_version,
};
use glam::Vec3A;

mod fixture {
    include!("fixtures/quad_navmesh.rs");
}

#[test]
fn bindings_report_the_pinned_recastnavigation_version() {
    assert_eq!(recastnavigation_version(), (1, 6, 0));
}

#[test]
fn invalid_tile_data_is_rejected() {
    assert!(matches!(
        NavMesh::from_tile_data(&[]),
        Err(Error::InvalidNavMeshData),
    ));
    assert!(matches!(
        NavMesh::from_tile_data(&[0; 128]),
        Err(Error::InvalidNavMeshData),
    ));
    assert!(matches!(
        NavMesh::from_tile_data(&fixture::QUAD_NAVMESH_TILE[..128]),
        Err(Error::InvalidNavMeshData),
    ));
}

#[test]
fn tiled_parameters_are_validated() {
    assert_eq!(
        NavMeshParams::new(
            Vec3A::ZERO,
            f32::NAN,
            16.0,
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ),
        Err(Error::InvalidNavMeshParameters),
    );
}

#[test]
fn query_filter_validates_area_costs() {
    assert_eq!(
        QueryFilter::new().with_area_cost(64, 1.0),
        Err(Error::InvalidArea(64)),
    );
    assert_eq!(
        QueryFilter::new().with_area_cost(0, f32::INFINITY),
        Err(Error::InvalidAreaCost),
    );
}

#[test]
fn single_tile_runtime_supports_nearest_path_and_raycast_queries() -> Result<(), Error> {
    let navmesh = NavMesh::from_tile_data(fixture::QUAD_NAVMESH_TILE)?;
    let query = navmesh.query()?;
    let filter = QueryFilter::new();
    let extents = Vec3A::new(2.0, 4.0, 2.0);

    let nearest = query
        .nearest_point(Vec3A::new(5.0, 2.0, 5.0), extents, &filter)?
        .ok_or(Error::StartPolygonNotFound)?;
    assert!(nearest.is_over_polygon());
    assert!(
        nearest
            .position()
            .abs_diff_eq(Vec3A::new(5.0, 0.0, 5.0), 0.0001)
    );

    let path = query.find_path(
        Vec3A::new(1.0, 0.0, 1.0),
        Vec3A::new(9.0, 0.0, 9.0),
        extents,
        &filter,
    )?;
    assert!(!path.is_partial());
    assert_eq!(path.corridor().len(), 1);
    assert_eq!(path.points().len(), 2);
    assert_eq!(path.points()[0].kind(), PathPointKind::Start);
    assert_eq!(path.points()[1].kind(), PathPointKind::End);

    let unobstructed = query.raycast(
        Vec3A::new(1.0, 0.0, 1.0),
        Vec3A::new(9.0, 0.0, 9.0),
        extents,
        &filter,
    )?;
    assert!(!unobstructed.hit());
    assert!((unobstructed.fraction() - 1.0).abs() < f32::EPSILON);

    let wall = query.raycast(
        Vec3A::new(5.0, 0.0, 5.0),
        Vec3A::new(12.0, 0.0, 5.0),
        extents,
        &filter,
    )?;
    assert!(wall.hit());
    assert!((wall.fraction() - (5.0 / 7.0)).abs() < 0.0001);
    assert_eq!(wall.visited().len(), 1);
    Ok(())
}

#[test]
fn polygon_handles_are_scoped_to_their_navmesh() -> Result<(), Error> {
    let first = NavMesh::from_tile_data(fixture::QUAD_NAVMESH_TILE)?;
    let second = NavMesh::from_tile_data(fixture::QUAD_NAVMESH_TILE)?;
    let first_query = first.query()?;
    let second_query = second.query()?;
    let nearest = first_query
        .nearest_point(
            Vec3A::new(5.0, 0.0, 5.0),
            Vec3A::splat(1.0),
            &QueryFilter::new(),
        )?
        .ok_or(Error::StartPolygonNotFound)?;

    assert_eq!(
        second_query.closest_point(nearest.polygon(), nearest.position()),
        Err(Error::WrongNavMesh),
    );
    Ok(())
}

#[test]
fn tiled_runtime_copies_and_owns_cooked_tiles() -> Result<(), Error> {
    let params = NavMeshParams::new(Vec3A::ZERO, 10.0, 10.0, NonZeroU32::MIN, NonZeroU32::MIN)?;
    let mut navmesh = NavMesh::tiled(params)?;
    let tile = navmesh.add_tile(fixture::QUAD_NAVMESH_TILE)?;

    assert!(navmesh.owns_tile(tile));
    assert_eq!(
        navmesh.add_tile(fixture::QUAD_NAVMESH_TILE),
        Err(Error::TileAlreadyOccupied),
    );
    Ok(())
}

#[test]
fn query_result_capacities_fail_explicitly() -> Result<(), Error> {
    let navmesh = NavMesh::from_tile_data(fixture::QUAD_NAVMESH_TILE)?;
    let query = navmesh
        .query_builder()
        .max_straight_path_points(NonZeroU32::MIN)
        .build()?;

    assert_eq!(
        query.find_path(
            Vec3A::new(1.0, 0.0, 1.0),
            Vec3A::new(9.0, 0.0, 9.0),
            Vec3A::splat(1.0),
            &QueryFilter::new(),
        ),
        Err(Error::StraightPathCapacityExceeded),
    );
    Ok(())
}
