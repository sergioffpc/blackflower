use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use blackflower::foreground::{self, ClientCapabilities, ForegroundConfig};
use blackflower::{ClientConnectionConfig, ConnectedClient};
use blackflower_assets::{AssetStore, AssetTrustStore};
use blackflower_harness::ClientHarnessConfig;
use blackflower_networking::{CompatibilityContract, ProtocolRevision, RequiredContentSetId};
use blackflower_networking_quic::{ClientEndpointConfig, ClientTrustRoot};
use blackflower_observability::{ObservabilityConfig, ObservabilityGuard, init};
use blackflower_process::{ShutdownToken, validate_foreground_terminal};
use clap::Parser;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject as _;

const DEFAULT_METRICS_PORT: u16 = 9_002;

#[derive(Debug, Parser)]
#[command(version, about = "Blackflower native client")]
struct Arguments {
    /// Run the native client with an interactive terminal dashboard.
    #[arg(long)]
    foreground: bool,

    /// Loopback address for client metrics and foreground polling.
    #[arg(long, default_value_t = default_metrics_address(), requires = "foreground")]
    metrics_bind_address: SocketAddr,

    /// Dedicated-server QUIC address.
    #[arg(long, default_value_t = default_server_address())]
    server_address: SocketAddr,

    /// DNS name authenticated by the server's TLS certificate.
    #[arg(long)]
    server_name: String,

    /// PEM certificate containing the current private service CA root.
    #[arg(long)]
    service_ca_certificate: PathBuf,

    /// Directory containing the locally installed signed cooked asset packages.
    #[arg(long)]
    asset_package_directory: PathBuf,

    /// Trusted Ed25519 asset-package public key PEM; repeat during key rotation.
    #[arg(long = "asset-trust-key")]
    asset_trust_keys: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let arguments = Arguments::parse();
    validate_arguments(&arguments)?;

    let mut config = ObservabilityConfig::client(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    if arguments.foreground {
        config = config
            .with_metrics_bind_address(Some(arguments.metrics_bind_address))
            .with_host_metrics(true)
            .with_default_foreground_logs();
    }
    let mut observability = init(&config).context("observability init failed")?;
    observability.report_log_pipeline_health();

    let connection_config = connection_config(&arguments)?;
    let network_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("client network runtime creation failed")?;
    let connected = network_runtime
        .block_on(ConnectedClient::connect(connection_config))
        .context("client connection failed")?;

    if arguments.foreground {
        run_with_foreground(&config, &mut observability, connected)?;
    } else {
        blackflower::run_connected(connected).context("connected client application failed")?;
    }

    observability.report_log_pipeline_health();
    Ok(())
}

fn validate_arguments(arguments: &Arguments) -> Result<()> {
    if !arguments.metrics_bind_address.ip().is_loopback() {
        bail!("--metrics-bind-address must be loopback");
    }
    validate_foreground_terminal(arguments.foreground)?;
    Ok(())
}

fn run_with_foreground(
    config: &ObservabilityConfig,
    observability: &mut ObservabilityGuard,
    connected: ConnectedClient,
) -> Result<()> {
    let metrics_address = config
        .metrics_bind_address()
        .context("client foreground metrics endpoint is disabled")?;
    let (log_receiver, log_control) = observability
        .take_foreground_logs()
        .context("client foreground log capture is disabled")?;
    let shutdown_requested = ShutdownToken::new();
    let foreground_shutdown = shutdown_requested.clone();
    let foreground_config = ForegroundConfig {
        service_name: config.service_name(),
        service_version: env!("CARGO_PKG_VERSION"),
        metrics_address,
        log_receiver,
        log_control,
        capabilities: ClientCapabilities::connected(),
        shutdown_requested: foreground_shutdown.clone(),
    };
    let foreground_thread = std::thread::Builder::new()
        .name("blackflower-client-foreground".to_owned())
        .spawn(move || {
            let result = foreground::run(foreground_config);
            foreground_shutdown.request();
            result
        })
        .context("client foreground thread startup failed")?;

    let client_result =
        blackflower::run_connected_with_shutdown(connected, shutdown_requested.shared_flag());
    shutdown_requested.request();
    let foreground_result = foreground_thread
        .join()
        .map_err(|_panic| anyhow::anyhow!("client foreground thread panicked"))?;
    client_result.context("client application failed")?;
    foreground_result.context("client foreground failed")
}

fn connection_config(arguments: &Arguments) -> Result<ClientConnectionConfig> {
    let current = read_single_certificate(&arguments.service_ca_certificate, "current service CA")?;
    let trust_store = load_asset_trust_store(&arguments.asset_trust_keys)?;
    let installed_assets = AssetStore::open_dir(&arguments.asset_package_directory, &trust_store)
        .with_context(|| {
        format!(
            "failed to validate asset packages in {}",
            arguments.asset_package_directory.display()
        )
    })?;
    let installed_content_set_id =
        RequiredContentSetId::from_bytes(*installed_assets.asset_set_hash().as_bytes());
    let bind_address = if arguments.server_address.is_ipv4() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
    } else {
        SocketAddr::new(std::net::Ipv6Addr::UNSPECIFIED.into(), 0)
    };
    Ok(ClientConnectionConfig {
        endpoint: ClientEndpointConfig {
            bind_address,
            server_address: arguments.server_address,
            server_name: arguments.server_name.clone(),
            trust_root: ClientTrustRoot { current },
        },
        harness: ClientHarnessConfig {
            compatibility: CompatibilityContract {
                protocol_revision: ProtocolRevision::V1,
            },
            installed_content_set_id,
        },
        installed_assets,
    })
}

fn load_asset_trust_store(paths: &[PathBuf]) -> Result<AssetTrustStore> {
    let mut trust_store = AssetTrustStore::new();
    for path in paths {
        let pem = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read asset trust key {}", path.display()))?;
        trust_store
            .trust_public_key_pem(&pem)
            .with_context(|| format!("failed to decode asset trust key {}", path.display()))?;
    }
    Ok(trust_store)
}

fn read_single_certificate(path: &Path, description: &str) -> Result<CertificateDer<'static>> {
    let mut certificates = CertificateDer::pem_file_iter(path).with_context(|| {
        format!(
            "failed to open {description} certificate {}",
            path.display()
        )
    })?;
    let certificate = certificates
        .next()
        .with_context(|| format!("{description} certificate file is empty"))?
        .with_context(|| format!("failed to decode {description} certificate"))?;
    if certificates.next().is_some() {
        bail!("{description} certificate file must contain exactly one certificate");
    }
    Ok(certificate)
}

const fn default_metrics_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_METRICS_PORT)
}

const fn default_server_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4_433)
}

#[cfg(test)]
#[path = "../tests/unit/arguments.rs"]
mod tests;
