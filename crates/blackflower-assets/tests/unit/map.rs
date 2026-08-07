use std::str::FromStr;

use super::*;

#[test]
fn map_asset_round_trips_player_model() -> Result<(), MapAssetError> {
    let model = AssetId::from_str("maps/bootstrap/player")?;
    let asset = MapAsset::new(model.clone());
    assert_eq!(
        MapAsset::from_bytes(&asset.to_bytes())?.player_model(),
        &model
    );
    Ok(())
}

#[test]
fn map_asset_rejects_trailing_bytes() -> Result<(), MapAssetError> {
    let mut bytes = MapAsset::new(AssetId::from_str("maps/bootstrap/player")?).to_bytes();
    bytes.push(0);
    assert!(matches!(
        MapAsset::from_bytes(&bytes),
        Err(MapAssetError::InvalidFormat(_))
    ));
    Ok(())
}
