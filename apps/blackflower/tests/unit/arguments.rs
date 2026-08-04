use clap::Parser as _;

use super::Arguments;

#[test]
fn native_mode_does_not_require_foreground_arguments() {
    assert!(Arguments::try_parse_from(["blackflower"]).is_ok());
}
