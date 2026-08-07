use super::{
    AcousticEnvironment, AcousticProbe, AcousticScene, AcousticZone, BakedDataIdentifier,
    BakedLayer, ProbeBatch, validate_authenticated_scene_contract,
};
use crate::Vec3A;
use blackflower_assets::AssetKind;

#[test]
fn scene_and_probe_formats_round_trip_and_reject_corruption() -> Result<(), crate::Error> {
    let scene = AcousticScene::encode(vec![1, 2, 3], 3, 1, 1)?;
    let decoded = AcousticScene::from_bytes(scene.bytes())?;
    assert_eq!(decoded.triangle_count(), 1);
    let mut corrupt = scene.bytes().to_vec();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(AcousticScene::from_bytes(&corrupt).is_err());

    let reverb = BakedDataIdentifier::reverb()?;
    let pathing = BakedDataIdentifier::dynamic_pathing()?;
    let batch = ProbeBatch::encode(
        "ground_floor".to_owned(),
        vec![AcousticProbe::new(Vec3A::Y, 2.0)?],
        vec![BakedLayer::new(reverb, 10), BakedLayer::new(pathing, 20)],
        vec![4, 5, 6],
    )?;
    let decoded = ProbeBatch::from_bytes(batch.bytes())?;
    assert_eq!(decoded.zone(), "ground_floor");
    assert_eq!(decoded.probes().len(), 1);
    assert_eq!(decoded.layers().len(), 2);
    Ok(())
}

#[test]
fn environment_is_sorted_and_strict() -> Result<(), crate::Error> {
    let environment = AcousticEnvironment::new(
        "levels/topology",
        vec![
            AcousticZone::new("upper", "levels/scene", "levels/upper")?,
            AcousticZone::new("ground", "levels/scene", "levels/ground")?,
        ],
    )?;
    assert_eq!(environment.zones()[0].id(), "ground");
    assert_eq!(environment.topology(), "levels/topology");
    assert_eq!(
        AcousticEnvironment::from_bytes(environment.bytes())?.zones(),
        environment.zones()
    );
    let mut unsupported_schema = environment.bytes().to_vec();
    unsupported_schema[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    let Err(error) = AcousticEnvironment::from_bytes(&unsupported_schema) else {
        return Err(crate::Error::InvalidAcousticAsset {
            format: "environment",
            reason: "unsupported environment schema was accepted",
        });
    };
    assert!(error.to_string().contains("schema is unsupported"));
    Ok(())
}

#[test]
fn native_scene_loading_requires_authenticated_kind_and_toolchain() {
    let expected = crate::steam_audio_acoustics_identity();
    assert!(validate_authenticated_scene_contract(AssetKind::AcousticScene, &expected).is_ok());
    assert!(matches!(
        validate_authenticated_scene_contract(AssetKind::Blob, &expected),
        Err(crate::Error::InvalidAcousticSceneAssetKind { .. })
    ));
    assert!(matches!(
        validate_authenticated_scene_contract(AssetKind::AcousticScene, "steam-audio/old"),
        Err(crate::Error::IncompatibleAcousticSceneToolchain { .. })
    ));
}
