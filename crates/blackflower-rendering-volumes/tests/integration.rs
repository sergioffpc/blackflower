use blackflower_rendering_volumes::{
    Error, GridClass, GridType, Vdb, openvdb_version, vdb_version,
};
use glam::{DVec3, IVec3};

const RAW_GRID: &[u8] = include_bytes!("fixtures/empty_float_grid.raw.nvdb");
const FILE_GRID: &[u8] = include_bytes!("fixtures/empty_float_grid.nvdb");

#[test]
fn bindings_report_pinned_versions() {
    assert_eq!(openvdb_version(), (13, 0, 0));
    assert_eq!(vdb_version(), (32, 9, 0));
}

#[test]
fn invalid_assets_are_rejected() {
    assert!(matches!(Vdb::from_bytes(&[]), Err(Error::InvalidAsset)));
    assert!(matches!(
        Vdb::from_bytes(&RAW_GRID[..128]),
        Err(Error::InvalidAsset)
    ));
}

#[test]
fn raw_grid_supports_metadata_transforms_and_float_sampling() -> Result<(), Error> {
    assert_send_sync::<Vdb>();

    let asset = Vdb::from_bytes(RAW_GRID)?;
    assert_eq!(asset.len(), 1);
    assert!(!asset.is_empty());
    assert_eq!(asset.grids().len(), 1);

    let grid = asset.grid(0).ok_or(Error::NativeContract)?;
    let metadata = grid.metadata();
    assert_eq!(metadata.name(), "density");
    assert_eq!(metadata.grid_type(), GridType::Float);
    assert_eq!(metadata.grid_class(), GridClass::FogVolume);
    assert_eq!(metadata.active_voxel_count(), 0);
    assert!(metadata.byte_size() > 0);
    assert_eq!(metadata.index_bounds(), None);
    assert_eq!(metadata.world_bounds(), None);
    assert_vec_close(metadata.voxel_size(), DVec3::splat(0.5));

    let world_origin = DVec3::new(1.0, 2.0, 3.0);
    assert_vec_close(grid.index_to_world(DVec3::ZERO)?, world_origin);
    assert_vec_close(grid.world_to_index(world_origin)?, DVec3::ZERO);
    assert_eq!(
        grid.world_to_index(DVec3::new(f64::NAN, 0.0, 0.0)),
        Err(Error::InvalidPosition)
    );

    let float_grid = grid.as_float().ok_or(Error::NativeContract)?;
    let voxel = float_grid.voxel(IVec3::new(42, -7, 9))?;
    assert!(!voxel.is_active());
    assert_float_close(voxel.value(), 3.25);
    assert_float_close(float_grid.sample_world(world_origin)?, 3.25);
    Ok(())
}

#[test]
fn uncompressed_file_container_loads() -> Result<(), Error> {
    let asset = Vdb::from_bytes(FILE_GRID)?;
    let grid = asset.grid(0).ok_or(Error::NativeContract)?;
    assert_eq!(grid.metadata().name(), "density");
    assert!(grid.as_float().is_some());
    Ok(())
}

fn assert_send_sync<T: Send + Sync>() {}

fn assert_float_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON);
}

fn assert_vec_close(actual: DVec3, expected: DVec3) {
    assert!((actual - expected).abs().max_element() <= f64::EPSILON);
}
