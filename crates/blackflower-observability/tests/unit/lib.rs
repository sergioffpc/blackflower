use std::num::NonZeroUsize;

use super::{ForegroundLogLevel, LogFormat, ObservabilityConfig};

#[test]
fn observability_enum_labels_are_derived_without_changing_the_contract() {
    assert_eq!(LogFormat::Compact.as_str(), "compact");
    assert_eq!(LogFormat::Pretty.as_str(), "pretty");
    assert_eq!(ForegroundLogLevel::Off.as_str(), "OFF");
    assert_eq!(ForegroundLogLevel::Error.as_str(), "ERROR");
    assert_eq!(ForegroundLogLevel::Warn.as_str(), "WARN");
    assert_eq!(ForegroundLogLevel::Info.as_str(), "INFO");
    assert_eq!(ForegroundLogLevel::Debug.as_str(), "DEBUG");
    assert_eq!(ForegroundLogLevel::Trace.as_str(), "TRACE");
}

#[test]
fn server_defaults_are_loopback_and_compact() {
    let config = ObservabilityConfig::server("blackflower-server", "0.1.0");

    assert_eq!(config.service_name(), "blackflower-server");
    assert_eq!(
        config.metrics_bind_address().map(|address| address.ip()),
        Some(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
    );
    assert_eq!(config.log_format, LogFormat::Compact);
    assert!(config.host_metrics_enabled());
}

#[test]
fn client_defaults_do_not_open_a_listener() {
    let config = ObservabilityConfig::client("blackflower", "0.1.0");

    assert_eq!(config.metrics_bind_address(), None);
    assert_eq!(config.log_format, LogFormat::Compact);
    assert!(!config.host_metrics_enabled());
}

#[test]
fn server_host_metrics_can_be_disabled_explicitly() {
    let config =
        ObservabilityConfig::server("blackflower-server", "0.1.0").with_host_metrics(false);

    assert!(!config.host_metrics_enabled());
}

#[test]
fn pretty_format_can_be_selected_explicitly() {
    let config =
        ObservabilityConfig::client("blackflower", "0.1.0").with_log_format(LogFormat::Pretty);

    assert_eq!(config.log_format, LogFormat::Pretty);
    assert_eq!(config.log_format.as_str(), "pretty");
}

#[test]
fn foreground_capture_is_explicit_and_bounded() {
    let config = ObservabilityConfig::server("blackflower-server", "0.1.0").with_foreground_logs(
        ForegroundLogLevel::Debug,
        NonZeroUsize::new(128).unwrap_or(NonZeroUsize::MIN),
    );

    assert!(config.foreground_logs_enabled());
    assert_eq!(config.foreground_log_capacity, Some(128));
    assert_eq!(config.foreground_log_level, ForegroundLogLevel::Debug);
    assert!(super::create_formatted_log_output(&config).is_none());
}

#[test]
fn foreground_dashboard_uses_the_shared_bounded_log_policy() {
    let config = ObservabilityConfig::client("blackflower", "0.1.0").with_default_foreground_logs();

    assert_eq!(config.foreground_log_capacity, Some(4_096));
    assert_eq!(config.foreground_log_level, ForegroundLogLevel::Info);
}
