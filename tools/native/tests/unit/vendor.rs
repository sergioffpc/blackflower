use super::*;

#[test]
fn vendor_names_are_derived_without_changing_the_contract() {
    let names = Vendor::ALL
        .iter()
        .copied()
        .map(Vendor::name)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "boost",
            "blast",
            "zlib",
            "blosc",
            "tbb",
            "openvdb",
            "embree",
            "flatbuffers",
            "pffft",
            "mysofa",
            "steam-audio",
            "flecs",
            "flow",
            "jolt",
            "ozz",
            "recast",
            "luau",
            "opus",
            "ktx",
            "slang",
        ],
    );
}

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
