use blackflower_observability::ForegroundLogLevel;
use clap::Parser as _;

use super::{Arguments, parse_log_level};

#[test]
fn parses_log_levels_case_insensitively() {
    assert_eq!(parse_log_level("DEBUG"), Ok(ForegroundLogLevel::Debug));
    assert!(parse_log_level("verbose").is_err());
}

#[test]
fn native_mode_does_not_require_foreground_arguments() {
    assert!(Arguments::try_parse_from(["blackflower"]).is_ok());
}
