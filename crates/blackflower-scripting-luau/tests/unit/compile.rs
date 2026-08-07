use blackflower_assets::AssetKind;

use super::validate_asset_contract;
use crate::Error;

#[test]
fn rejects_authenticated_assets_with_a_non_luau_kind() {
    assert_eq!(
        validate_asset_contract(AssetKind::Blob, "luau/0.731.0"),
        Err(Error::InvalidBytecodeAssetKind {
            actual: AssetKind::Blob,
        })
    );
}

#[test]
fn rejects_authenticated_bytecode_for_another_luau_toolchain() {
    assert_eq!(
        validate_asset_contract(AssetKind::LuauBytecode, "luau/0.730.0"),
        Err(Error::IncompatibleBytecodeToolchain {
            expected: "luau/0.731.0".to_owned(),
            actual: "luau/0.730.0".to_owned(),
        })
    );
}

#[test]
fn accepts_authenticated_bytecode_for_the_linked_luau_toolchain() {
    assert_eq!(
        validate_asset_contract(AssetKind::LuauBytecode, "luau/0.731.0"),
        Ok(())
    );
}
