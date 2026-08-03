use std::path::Path;

use super::is_direct_package;

#[test]
fn package_filter_is_non_recursive_and_case_sensitive() {
    let directory = Path::new("/packages");

    assert!(is_direct_package(
        directory,
        Path::new("/packages/pak000.squashfs")
    ));
    assert!(!is_direct_package(
        directory,
        Path::new("/packages/nested/pak900.squashfs")
    ));
    assert!(!is_direct_package(
        directory,
        Path::new("/packages/pak000.SQUASHFS")
    ));
    assert!(!is_direct_package(
        directory,
        Path::new("/packages/readme.txt")
    ));
}
