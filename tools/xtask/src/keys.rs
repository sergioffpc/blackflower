use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail, ensure};
use clap::Subcommand;

const DEFAULT_OUTPUT_DIRECTORY: &str = ".local-network";
const DEFAULT_SERVER_NAME: &str = "localhost";
const CERTIFICATE_VALIDITY_DAYS: &str = "1";

#[derive(Debug, clap::Args)]
pub(crate) struct KeysArgs {
    #[command(subcommand)]
    command: KeysCommand,
}

#[derive(Debug, Subcommand)]
enum KeysCommand {
    /// Generate a local CA, server certificate, and asset-signing key pair.
    Generate(GenerateArgs),
}

#[derive(Debug, clap::Args)]
struct GenerateArgs {
    /// Directory that will receive the generated credentials.
    #[arg(long, default_value = DEFAULT_OUTPUT_DIRECTORY)]
    output: PathBuf,

    /// DNS name placed in the server certificate subject alternative name.
    #[arg(long, default_value = DEFAULT_SERVER_NAME)]
    server_name: String,
}

pub(crate) fn run_keys(workspace_root: &Path, args: KeysArgs) -> anyhow::Result<()> {
    match args.command {
        KeysCommand::Generate(args) => generate(workspace_root, &args, Path::new("openssl")),
    }
}

fn generate(workspace_root: &Path, args: &GenerateArgs, openssl: &Path) -> anyhow::Result<()> {
    validate_server_name(&args.server_name)?;
    let output = resolve_output(workspace_root, &args.output);
    ensure!(
        !output.exists(),
        "refusing to replace existing key directory `{}`",
        output.display()
    );
    let staging = create_staging_directory(&output)?;
    let paths = KeyPaths::new(staging.path());
    generate_staged_credentials(openssl, &paths, &args.server_name)?;
    publish_credentials(staging, &output)?;
    report_generated_credentials(&output);
    Ok(())
}

fn create_staging_directory(output: &Path) -> anyhow::Result<tempfile::TempDir> {
    let parent = output.parent().with_context(|| {
        format!(
            "key output directory `{}` has no parent directory",
            output.display()
        )
    })?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create key output parent directory `{}`",
            parent.display()
        )
    })?;
    tempfile::Builder::new()
        .prefix(".blackflower-keys-")
        .tempdir_in(parent)
        .with_context(|| {
            format!(
                "failed to create temporary key directory in `{}`",
                parent.display()
            )
        })
}

fn generate_staged_credentials(
    openssl: &Path,
    paths: &KeyPaths,
    server_name: &str,
) -> anyhow::Result<()> {
    generate_service_ca(openssl, paths)?;
    generate_server_certificate(openssl, paths, server_name)?;
    generate_asset_signing_keys(openssl, paths)?;
    write_server_chain(paths)?;
    validate_generated_credentials(openssl, paths, server_name)?;
    remove_intermediate_files(paths)?;
    restrict_private_key_permissions(paths)
}

fn publish_credentials(staging: tempfile::TempDir, output: &Path) -> anyhow::Result<()> {
    let staging_path = staging.keep();
    if let Err(source) = fs::rename(&staging_path, output) {
        let _cleanup_result = fs::remove_dir_all(&staging_path);
        return Err(source).with_context(|| {
            format!(
                "failed to publish generated keys to `{}`; the destination may have appeared while OpenSSL was running",
                output.display()
            )
        });
    }
    Ok(())
}

fn report_generated_credentials(output: &Path) {
    println!(
        "generated local TLS and asset-signing credentials in {}",
        output.display()
    );
    println!(
        "server certificate: {}",
        output.join("server-chain.pem").display()
    );
    println!(
        "server private key: {}",
        output.join("server-key.pem").display()
    );
    println!(
        "client service CA: {}",
        output.join("service-ca.pem").display()
    );
    println!(
        "asset signing key: {}",
        output.join("asset-signing-key.pem").display()
    );
    println!(
        "asset trust key: {}",
        output.join("asset-signing-public.pem").display()
    );
}

fn resolve_output(workspace_root: &Path, output: &Path) -> PathBuf {
    if output.is_absolute() {
        output.to_path_buf()
    } else {
        workspace_root.join(output)
    }
}

fn validate_server_name(server_name: &str) -> anyhow::Result<()> {
    ensure!(
        !server_name.is_empty() && server_name.len() <= 253,
        "server name must contain between 1 and 253 ASCII characters"
    );
    for label in server_name.split('.') {
        ensure!(
            !label.is_empty() && label.len() <= 63,
            "server name contains an empty or overlong DNS label"
        );
        ensure!(
            !label.starts_with('-') && !label.ends_with('-'),
            "server name DNS labels cannot start or end with `-`"
        );
        ensure!(
            label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
            "server name must be a DNS name containing only ASCII letters, digits, `-`, and `.`"
        );
    }
    Ok(())
}

fn generate_service_ca(openssl: &Path, paths: &KeyPaths) -> anyhow::Result<()> {
    run_openssl(openssl, "generate the local service CA", |command| {
        command
            .args(["req", "-x509", "-newkey", "ed25519", "-nodes"])
            .arg("-keyout")
            .arg(&paths.service_ca_key)
            .arg("-out")
            .arg(&paths.service_ca_certificate)
            .args([
                "-days",
                CERTIFICATE_VALIDITY_DAYS,
                "-subj",
                "/CN=Blackflower local CA",
                "-addext",
                "basicConstraints=critical,CA:TRUE",
                "-addext",
                "keyUsage=critical,keyCertSign,cRLSign",
            ]);
    })
}

fn generate_server_certificate(
    openssl: &Path,
    paths: &KeyPaths,
    server_name: &str,
) -> anyhow::Result<()> {
    run_openssl(
        openssl,
        "generate the local server key and CSR",
        |command| {
            command
                .args(["req", "-new", "-newkey", "ed25519", "-nodes"])
                .arg("-keyout")
                .arg(&paths.server_key)
                .arg("-out")
                .arg(&paths.server_request)
                .arg("-subj")
                .arg(format!("/CN={server_name}"));
        },
    )?;

    fs::write(
        &paths.server_extensions,
        format!(
            "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nsubjectAltName=DNS:{server_name}\nextendedKeyUsage=serverAuth\n"
        ),
    )
    .with_context(|| {
        format!(
            "failed to write server certificate extensions `{}`",
            paths.server_extensions.display()
        )
    })?;

    run_openssl(openssl, "sign the local server certificate", |command| {
        command
            .args(["x509", "-req"])
            .arg("-in")
            .arg(&paths.server_request)
            .arg("-CA")
            .arg(&paths.service_ca_certificate)
            .arg("-CAkey")
            .arg(&paths.service_ca_key)
            .arg("-CAserial")
            .arg(&paths.service_ca_serial)
            .arg("-CAcreateserial")
            .arg("-out")
            .arg(&paths.server_leaf_certificate)
            .args(["-days", CERTIFICATE_VALIDITY_DAYS])
            .arg("-extfile")
            .arg(&paths.server_extensions);
    })
}

fn generate_asset_signing_keys(openssl: &Path, paths: &KeyPaths) -> anyhow::Result<()> {
    run_openssl(
        openssl,
        "generate the asset-signing private key",
        |command| {
            command
                .args(["genpkey", "-algorithm", "ED25519", "-out"])
                .arg(&paths.asset_signing_key);
        },
    )?;
    run_openssl(openssl, "derive the asset-signing public key", |command| {
        command
            .args(["pkey", "-in"])
            .arg(&paths.asset_signing_key)
            .args(["-pubout", "-out"])
            .arg(&paths.asset_signing_public_key);
    })
}

fn write_server_chain(paths: &KeyPaths) -> anyhow::Result<()> {
    let mut chain = fs::File::create(&paths.server_chain).with_context(|| {
        format!(
            "failed to create server certificate chain `{}`",
            paths.server_chain.display()
        )
    })?;
    for certificate in [
        &paths.server_leaf_certificate,
        &paths.service_ca_certificate,
    ] {
        let mut source = fs::File::open(certificate).with_context(|| {
            format!(
                "failed to open certificate `{}` while constructing the server chain",
                certificate.display()
            )
        })?;
        io::copy(&mut source, &mut chain).with_context(|| {
            format!(
                "failed to append certificate `{}` to `{}`",
                certificate.display(),
                paths.server_chain.display()
            )
        })?;
    }
    Ok(())
}

fn validate_generated_credentials(
    openssl: &Path,
    paths: &KeyPaths,
    server_name: &str,
) -> anyhow::Result<()> {
    for key in [
        &paths.service_ca_key,
        &paths.server_key,
        &paths.asset_signing_key,
    ] {
        run_openssl(openssl, "validate a generated private key", |command| {
            command
                .arg("pkey")
                .arg("-in")
                .arg(key)
                .args(["-check", "-noout"]);
        })?;
    }
    run_openssl(openssl, "validate the asset trust key", |command| {
        command
            .args(["pkey", "-pubin", "-in"])
            .arg(&paths.asset_signing_public_key)
            .arg("-noout");
    })?;
    run_openssl(
        openssl,
        "verify the generated server certificate",
        |command| {
            command
                .arg("verify")
                .arg("-CAfile")
                .arg(&paths.service_ca_certificate)
                .arg("-verify_hostname")
                .arg(server_name)
                .arg(&paths.server_leaf_certificate);
        },
    )
}

fn remove_intermediate_files(paths: &KeyPaths) -> anyhow::Result<()> {
    for path in [
        &paths.server_request,
        &paths.server_extensions,
        &paths.service_ca_serial,
    ] {
        fs::remove_file(path)
            .with_context(|| format!("failed to remove intermediate file `{}`", path.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_private_key_permissions(paths: &KeyPaths) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(paths.root, fs::Permissions::from_mode(0o700)).with_context(|| {
        format!(
            "failed to restrict key directory permissions for `{}`",
            paths.root.display()
        )
    })?;
    for key in [
        &paths.service_ca_key,
        &paths.server_key,
        &paths.asset_signing_key,
    ] {
        fs::set_permissions(key, fs::Permissions::from_mode(0o600)).with_context(|| {
            format!(
                "failed to restrict private key permissions for `{}`",
                key.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn restrict_private_key_permissions(_paths: &KeyPaths) -> anyhow::Result<()> {
    Ok(())
}

fn run_openssl(
    openssl: &Path,
    operation: &str,
    configure: impl FnOnce(&mut Command),
) -> anyhow::Result<()> {
    let mut command = Command::new(openssl);
    configure(&mut command);
    let output = command.output().with_context(|| {
        format!(
            "failed to execute `{}` to {operation}; install OpenSSL and ensure it is available on PATH",
            openssl.display()
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "OpenSSL failed to {operation} with status {}: {}",
            output.status,
            stderr.trim()
        );
    }
    Ok(())
}

struct KeyPaths<'a> {
    root: &'a Path,
    service_ca_key: PathBuf,
    service_ca_certificate: PathBuf,
    service_ca_serial: PathBuf,
    server_key: PathBuf,
    server_request: PathBuf,
    server_extensions: PathBuf,
    server_leaf_certificate: PathBuf,
    server_chain: PathBuf,
    asset_signing_key: PathBuf,
    asset_signing_public_key: PathBuf,
}

impl<'a> KeyPaths<'a> {
    fn new(root: &'a Path) -> Self {
        Self {
            root,
            service_ca_key: root.join("service-ca-key.pem"),
            service_ca_certificate: root.join("service-ca.pem"),
            service_ca_serial: root.join("service-ca.srl"),
            server_key: root.join("server-key.pem"),
            server_request: root.join("server.csr"),
            server_extensions: root.join("server.ext"),
            server_leaf_certificate: root.join("server-leaf.pem"),
            server_chain: root.join("server-chain.pem"),
            asset_signing_key: root.join("asset-signing-key.pem"),
            asset_signing_public_key: root.join("asset-signing-public.pem"),
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/keys.rs"]
mod tests;
