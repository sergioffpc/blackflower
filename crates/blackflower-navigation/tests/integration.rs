use std::num::NonZeroU32;

use blackflower_navigation::{
    Error, NavAgentProfile, NavAgentProfileId, NavMesh, NavMeshAsset, NavMeshParams,
    NavigationArea, NavigationAreaKey, NavigationBuildSettings, NavigationTile, PathPointKind,
    QueryFilter, recastnavigation_version,
};
use bytes::Bytes;
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
fn tile_internal_indices_are_validated_before_native_loading()
-> Result<(), Box<dyn std::error::Error>> {
    let mut corrupt = fixture::QUAD_NAVMESH_TILE.to_vec();
    let header_bytes = 100;
    let vertex_count = u32::from_le_bytes(corrupt[28..32].try_into()?);
    let first_polygon = header_bytes + usize::try_from(vertex_count)? * 12;
    corrupt[first_polygon + 4..first_polygon + 6].copy_from_slice(&u16::MAX.to_le_bytes());

    assert!(matches!(
        NavMesh::from_tile_data(&corrupt),
        Err(Error::InvalidNavMeshData),
    ));
    Ok(())
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
fn tiled_runtime_replaces_and_removes_cooked_tiles() -> Result<(), Error> {
    let params = NavMeshParams::new(Vec3A::ZERO, 10.0, 10.0, NonZeroU32::MIN, NonZeroU32::MIN)?;
    let mut navmesh = NavMesh::tiled(params)?;
    let tile = navmesh.add_tile(fixture::QUAD_NAVMESH_TILE)?;

    let replaced = navmesh.replace_tile(tile, fixture::QUAD_NAVMESH_TILE)?;
    assert_eq!(replaced, tile);
    assert!(
        navmesh
            .query()?
            .nearest_point(
                Vec3A::new(0.5, 0.0, 0.5),
                Vec3A::splat(1.0),
                &QueryFilter::default(),
            )?
            .is_some()
    );

    assert!(matches!(
        navmesh.replace_tile(tile, &[0; 128]),
        Err(Error::InvalidNavMeshData)
    ));
    assert!(
        navmesh
            .query()?
            .nearest_point(
                Vec3A::new(0.5, 0.0, 0.5),
                Vec3A::splat(1.0),
                &QueryFilter::default(),
            )?
            .is_some()
    );

    navmesh.remove_tile(tile)?;
    assert!(
        navmesh
            .query()?
            .nearest_point(
                Vec3A::new(0.5, 0.0, 0.5),
                Vec3A::splat(1.0),
                &QueryFilter::default(),
            )?
            .is_none()
    );
    assert!(matches!(navmesh.remove_tile(tile), Err(Error::InvalidTile)));
    Ok(())
}

#[test]
fn tile_mutation_rejects_handles_from_another_navmesh() -> Result<(), Error> {
    let params = NavMeshParams::new(Vec3A::ZERO, 10.0, 10.0, NonZeroU32::MIN, NonZeroU32::MIN)?;
    let mut first = NavMesh::tiled(params)?;
    let tile = first.add_tile(fixture::QUAD_NAVMESH_TILE)?;
    let mut second = NavMesh::tiled(params)?;

    assert!(matches!(second.remove_tile(tile), Err(Error::WrongNavMesh)));
    assert!(matches!(
        second.replace_tile(tile, fixture::QUAD_NAVMESH_TILE),
        Err(Error::WrongNavMesh)
    ));
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

#[test]
fn bfnav_round_trip_instantiates_native_filter_and_tiles() -> Result<(), Error> {
    let params = NavMeshParams::new(Vec3A::ZERO, 10.0, 10.0, NonZeroU32::MIN, NonZeroU32::MIN)?;
    let asset = NavMeshAsset::new(
        NavAgentProfile::new(NavAgentProfileId::new("humanoid")?, 1.8, 0.35, 0.4, 45.0)?,
        NavigationBuildSettings::new(0.2, 0.1, 64, 8, 20, 12.0, 1.3, 6, 6.0, 1.0)?,
        params,
        vec![
            NavigationArea::new(0, NavigationAreaKey::new("ground")?, true, Some(1.0))?,
            NavigationArea::new(1, NavigationAreaKey::new("water")?, false, None)?,
        ],
        vec![NavigationTile::new(
            0,
            0,
            0,
            Bytes::from_static(fixture::QUAD_NAVMESH_TILE),
        )?],
    )?;
    let decoded = NavMeshAsset::from_bytes(asset.bytes().clone())?;
    assert_eq!(decoded.agent().id().as_str(), "humanoid");
    assert_eq!(decoded.areas()[1].cost().to_bits(), 0.0_f32.to_bits());
    let filter = decoded.query_filter()?;
    assert_eq!(filter.include_flags(), 1);
    assert_eq!(filter.exclude_flags(), 0);
    assert_eq!(filter.area_cost(0)?.to_bits(), 1.0_f32.to_bits());
    assert_eq!(filter.area_cost(1)?.to_bits(), 1.0_f32.to_bits());

    let navmesh = decoded.instantiate()?;
    let path = navmesh.query()?.find_path(
        Vec3A::new(1.0, 0.0, 1.0),
        Vec3A::new(9.0, 0.0, 9.0),
        Vec3A::splat(2.0),
        &filter,
    )?;
    assert!(!path.is_partial());
    Ok(())
}

#[test]
fn bfnav_rejects_corrupted_embedded_identity() -> Result<(), Error> {
    let params = NavMeshParams::new(Vec3A::ZERO, 10.0, 10.0, NonZeroU32::MIN, NonZeroU32::MIN)?;
    let asset = NavMeshAsset::new(
        NavAgentProfile::new(NavAgentProfileId::new("humanoid")?, 1.8, 0.35, 0.4, 45.0)?,
        NavigationBuildSettings::new(0.2, 0.1, 64, 8, 20, 12.0, 1.3, 6, 6.0, 1.0)?,
        params,
        vec![NavigationArea::new(
            0,
            NavigationAreaKey::new("ground")?,
            true,
            Some(1.0),
        )?],
        vec![NavigationTile::new(
            0,
            0,
            0,
            Bytes::from_static(fixture::QUAD_NAVMESH_TILE),
        )?],
    )?;
    let mut bytes = asset.bytes().to_vec();
    const FIRST_AGENT_HASH_BYTE: usize = 60;
    bytes[FIRST_AGENT_HASH_BYTE] ^= 0xff;
    assert!(NavMeshAsset::from_bytes(Bytes::from(bytes)).is_err());
    Ok(())
}
