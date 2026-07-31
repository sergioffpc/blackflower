use core::str::FromStr;

use super::{AssetId, PackageName, ProfileName};

#[test]
fn accepts_canonical_ids() -> Result<(), Box<dyn std::error::Error>> {
    let id = AssetId::from_str("characters/soldier.mesh_v2")?;
    assert_eq!(id.as_str(), "characters/soldier.mesh_v2");
    Ok(())
}

#[test]
fn rejects_non_canonical_ids() {
    for invalid in [
        "",
        "/root",
        "root/",
        "root//child",
        "root/../child",
        "Root/child",
        "root/café",
    ] {
        assert!(
            AssetId::from_str(invalid).is_err(),
            "{invalid} should be rejected"
        );
    }
}

#[test]
fn package_names_have_portable_filenames() -> Result<(), Box<dyn std::error::Error>> {
    let name = PackageName::from_str("pak900-hotfix")?;
    assert_eq!(name.file_name(), "pak900-hotfix.squashfs");
    assert_eq!(PackageName::from_file_name(&name.file_name())?, name);
    Ok(())
}

#[test]
fn rejects_non_portable_package_names() {
    for invalid in ["", "-pak", "Pak000", "pák000", &"p".repeat(65)] {
        assert!(
            PackageName::from_str(invalid).is_err(),
            "{invalid} should be rejected"
        );
    }
}

#[test]
fn profile_names_use_the_portable_filename_grammar() -> Result<(), Box<dyn std::error::Error>> {
    let name = ProfileName::from_str("desktop-production")?;
    assert_eq!(name.as_str(), "desktop-production");
    for invalid in ["", "-desktop", "Desktop", "désktop", &"p".repeat(65)] {
        assert!(
            ProfileName::from_str(invalid).is_err(),
            "{invalid} should be rejected"
        );
    }
    Ok(())
}
