use super::*;

#[test]
fn native_build_lock_rejects_a_concurrent_owner() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let configuration = Configuration::new(
        "aarch64-apple-darwin".to_owned(),
        CargoProfile::Debug,
        false,
    );
    let _owner = acquire_build_lock(directory.path(), &configuration)?;
    let lock_path = directory
        .path()
        .join(configuration.relative_directory())
        .join(".blackflower-native-build.lock");
    let contender = OpenOptions::new().read(true).write(true).open(lock_path)?;

    let error = match contender.try_lock() {
        Ok(()) => bail!("concurrent lock unexpectedly succeeded"),
        Err(error) => error,
    };
    assert!(matches!(error, TryLockError::WouldBlock));
    Ok(())
}
