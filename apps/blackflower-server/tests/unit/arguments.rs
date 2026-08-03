use blackflower_observability::ForegroundLogLevel;

use super::parse_log_level;

#[test]
fn parses_log_levels_case_insensitively() {
    assert_eq!(parse_log_level("DEBUG"), Ok(ForegroundLogLevel::Debug));
    assert!(parse_log_level("verbose").is_err());
}
