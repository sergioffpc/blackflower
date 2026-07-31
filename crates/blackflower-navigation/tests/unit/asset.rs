use super::*;

#[test]
fn area_keys_and_blocked_costs_are_strict() -> Result<(), Error> {
    assert!(NavigationAreaKey::new("Water").is_err());
    let key = NavigationAreaKey::new("water")?;
    assert!(NavigationArea::new(0, key.clone(), false, Some(1.0)).is_err());
    assert_eq!(
        NavigationArea::new(0, key, false, None)?.cost().to_bits(),
        0.0_f32.to_bits()
    );
    Ok(())
}

#[test]
fn build_hash_changes_with_every_setting() -> Result<(), Error> {
    let first = build(1.0)?;
    let second = build(1.1)?;
    assert_ne!(first.settings_hash(), second.settings_hash());
    Ok(())
}

fn build(error: f32) -> Result<NavigationBuildSettings, Error> {
    NavigationBuildSettings::new(0.2, 0.1, 64, 8, 20, 12.0, error, 6, 6.0, 1.0)
}
