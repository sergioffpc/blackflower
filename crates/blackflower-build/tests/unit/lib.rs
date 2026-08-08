use super::CargoProfile;

#[test]
fn cargo_profile_cli_names_are_derived_without_changing_the_contract() {
    assert_eq!(CargoProfile::Debug.cli_name(), "debug");
    assert_eq!(CargoProfile::Release.cli_name(), "release");
}
