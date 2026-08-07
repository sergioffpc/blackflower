use clap::Parser as _;

use super::Arguments;

#[test]
fn connection_inputs_are_always_required() {
    assert!(Arguments::try_parse_from(["blackflower"]).is_err());
}

#[test]
fn complete_authenticated_connection_defaults_to_localhost() -> Result<(), clap::Error> {
    let arguments = Arguments::try_parse_from([
        "blackflower",
        "--server-name",
        "localhost",
        "--service-ca-certificate",
        "service-ca.pem",
        "--asset-package-directory",
        "target/assets/packages/debug",
    ])?;

    assert_eq!(arguments.server_address.to_string(), "127.0.0.1:4433");
    Ok(())
}

#[test]
fn dedicated_server_address_can_override_localhost() -> Result<(), clap::Error> {
    let arguments = Arguments::try_parse_from([
        "blackflower",
        "--server-address",
        "192.0.2.10:8443",
        "--server-name",
        "game.example.test",
        "--service-ca-certificate",
        "service-ca.pem",
        "--asset-package-directory",
        "target/assets/packages/release",
    ])?;

    assert_eq!(arguments.server_address.to_string(), "192.0.2.10:8443");
    Ok(())
}
