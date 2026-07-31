use super::{LogFormat, ObservabilityConfig};

#[test]
fn server_defaults_are_loopback_and_compact() {
    let config = ObservabilityConfig::server("blackflower-server", "0.1.0");

    assert_eq!(config.service_name(), "blackflower-server");
    assert_eq!(
        config.metrics_bind_address().map(|address| address.ip()),
        Some(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
    );
    assert_eq!(config.log_format, LogFormat::Compact);
}

#[test]
fn client_defaults_do_not_open_a_listener() {
    let config = ObservabilityConfig::client("blackflower", "0.1.0");

    assert_eq!(config.metrics_bind_address(), None);
    assert_eq!(config.log_format, LogFormat::Compact);
}

#[test]
fn pretty_format_can_be_selected_explicitly() {
    let config =
        ObservabilityConfig::client("blackflower", "0.1.0").with_log_format(LogFormat::Pretty);

    assert_eq!(config.log_format, LogFormat::Pretty);
    assert_eq!(config.log_format.as_str(), "pretty");
}
