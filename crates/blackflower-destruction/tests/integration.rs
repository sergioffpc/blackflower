use blackflower_destruction::{
    Asset, BondDesc, ChunkDesc, Error, Family, FractureCommand, GraphNodeId, StressSettings,
    blast_version, stress_supported,
};
use glam::Vec3A;

fn two_chunk_asset() -> Result<Asset, Error> {
    let chunks = [
        ChunkDesc {
            centroid: Vec3A::new(-0.5, 0.0, 0.0),
            volume: 1.0,
            parent: None,
            support: true,
            user_data: 10,
        },
        ChunkDesc {
            centroid: Vec3A::new(0.5, 0.0, 0.0),
            volume: 1.0,
            parent: None,
            support: true,
            user_data: 11,
        },
    ];
    let bonds = [BondDesc {
        normal: Vec3A::X,
        area: 1.0,
        centroid: Vec3A::ZERO,
        chunks: [Some(0), Some(1)],
        user_data: 20,
    }];
    Asset::new(&chunks, &bonds)
}

#[test]
fn creates_asset_family_and_splits_a_broken_bond() -> Result<(), Error> {
    assert_eq!(blast_version(), "5.0.6");
    let asset = two_chunk_asset()?;
    assert_eq!(asset.chunk_count(), 2);
    assert_eq!(asset.bond_count(), 1);
    assert_eq!(asset.support_chunk_count(), 2);
    assert_eq!(asset.graph_node_count(), 2);

    let mut family = Family::new(&asset, 1.0, 1.0)?;
    let actors = family.actors()?;
    assert_eq!(actors.len(), 1);
    assert_eq!(family.visible_chunks(actors[0])?.len(), 2);

    let events = family.apply_fracture(
        actors[0],
        &[FractureCommand::Bond {
            first: GraphNodeId::new(0),
            second: GraphNodeId::new(1),
            damage: 2.0,
        }],
    )?;
    assert_eq!(events.len(), 1);
    let replacements = family.split_actor(actors[0])?;
    assert_eq!(replacements.len(), 2);
    assert_eq!(family.actors()?.len(), 2);
    Ok(())
}

#[test]
fn rejects_out_of_range_fracture_and_stress_targets() -> Result<(), Error> {
    let asset = two_chunk_asset()?;
    let mut family = Family::new(&asset, 1.0, 1.0)?;
    let actor = family.actors()?[0];
    assert_eq!(
        family.apply_fracture(
            actor,
            &[FractureCommand::Chunk {
                chunk_index: 2,
                damage: 1.0,
            }],
        ),
        Err(Error::InvalidFractureTarget)
    );
    assert_eq!(
        family.add_stress_force(
            GraphNodeId::new(2),
            Vec3A::ZERO,
            blackflower_destruction::ForceMode::Force,
        ),
        Err(Error::GraphNodeNotFound)
    );
    Ok(())
}

#[test]
fn rejects_invalid_authored_geometry_before_native_creation() {
    let invalid = [ChunkDesc {
        centroid: Vec3A::ZERO,
        volume: 0.0,
        parent: None,
        support: true,
        user_data: 0,
    }];
    assert!(matches!(
        Asset::new(&invalid, &[]),
        Err(Error::InvalidChunk)
    ));
}

#[test]
fn reports_upstream_stress_availability_explicitly() -> Result<(), Error> {
    let asset = two_chunk_asset()?;
    let mut family = Family::new(&asset, 1.0, 1.0)?;
    let result = family.enable_stress(StressSettings::default(), 1_000.0);
    if stress_supported() {
        assert!(result.is_ok());
    } else {
        assert_eq!(result, Err(Error::StressUnavailable));
    }
    Ok(())
}
