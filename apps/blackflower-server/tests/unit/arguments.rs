use clap::Parser as _;

use super::{Arguments, validate_arguments};

#[test]
fn simulation_only_mode_needs_no_network_arguments() {
    assert!(Arguments::try_parse_from(["blackflower-server"]).is_ok());
}

#[test]
fn listen_address_requires_complete_authenticated_endpoint_arguments() {
    assert!(
        Arguments::try_parse_from(["blackflower-server", "--listen-address", "127.0.0.1:4433"])
            .is_err()
    );
}

#[test]
fn local_authority_rejects_non_loopback_listen_address() -> Result<(), clap::Error> {
    let arguments = Arguments::try_parse_from([
        "blackflower-server",
        "--listen-address",
        "0.0.0.0:4433",
        "--tls-certificate",
        "server.pem",
        "--tls-private-key",
        "server-key.pem",
        "--map-id",
        "maps/test",
        "--asset-package-directory",
        "target/assets/packages/debug",
    ])?;

    assert!(validate_arguments(&arguments).is_err());
    Ok(())
}
