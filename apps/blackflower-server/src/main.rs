use std::io::IsTerminal as _;
use std::net::SocketAddr;
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use blackflower_assets::{AssetStore, AssetTrustStore};
use blackflower_networking::{
    BudgetTier, CompatibilityContract, ContentManifest, MapId, ProtocolRevision,
    RequiredContentSetId,
};
use blackflower_networking_quic::{
    AdmissionLimits, QuicServer, ServerEndpointConfig, ServerTlsConfig,
};
use blackflower_observability::{ObservabilityConfig, ObservabilityGuard, init};
use blackflower_server::foreground::{self, ForegroundConfig};
use blackflower_server::{
    DedicatedServerNetwork, LoopbackSessionAuthority, ServerNetworkRuntime, SimulationHost,
    SimulationStatus,
};
use clap::Parser;
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

const FOREGROUND_LOG_CAPACITY: usize = 4_096;

#[derive(Debug, Parser)]
#[command(version, about = "Blackflower authoritative server")]
struct Arguments {
    /// Run the interactive foreground metrics and logs dashboard.
    #[arg(long)]
    foreground: bool,

    /// QUIC listen address; omitting it runs only the authoritative simulation.
    #[arg(
        long,
        requires_all = [
            "tls_certificate",
            "tls_private_key",
            "map_id",
            "asset_package_directory"
        ]
    )]
    listen_address: Option<SocketAddr>,

    /// PEM leaf-first TLS certificate chain presented by the QUIC server.
    #[arg(long, requires = "listen_address")]
    tls_certificate: Option<PathBuf>,

    /// PEM private key corresponding to the server leaf certificate.
    #[arg(long, requires = "listen_address")]
    tls_private_key: Option<PathBuf>,

    /// Logical map selected and announced by the server.
    #[arg(long, requires = "listen_address")]
    map_id: Option<MapId>,

    /// Directory containing the server's signed cooked asset packages.
    #[arg(long, requires = "listen_address")]
    asset_package_directory: Option<PathBuf>,

    /// Trusted Ed25519 asset-package public key PEM; repeat during key rotation.
    #[arg(long = "asset-trust-key", requires = "listen_address")]
    asset_trust_keys: Vec<PathBuf>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let arguments = Arguments::parse();
    validate_arguments(&arguments)?;

    let mut config = ObservabilityConfig::server(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    if arguments.foreground {
        let capacity = NonZeroUsize::new(FOREGROUND_LOG_CAPACITY)
            .context("foreground log capacity must be non-zero")?;
        config = config.with_foreground_logs(Default::default(), capacity);
    }
    let metrics_address = config.metrics_bind_address();
    let mut observability = init(&config).context("observability init failed")?;
    observability.report_health();

    let simulation = SimulationHost::spawn().context("simulation host startup failed")?;
    let stop = Arc::new(AtomicBool::new(false));
    let network_runtime = network_runtime(&arguments, simulation.status())?;
    let network_task = network_runtime.map(|runtime| {
        let network_stop = Arc::clone(&stop);
        tokio::spawn(async move { runtime.run(network_stop).await })
    });
    let application_result = run_application(
        &arguments,
        &config,
        metrics_address,
        &mut observability,
        Arc::clone(&stop),
    )
    .await;
    stop.store(true, Ordering::Release);
    let network_result = if let Some(task) = network_task {
        Some(task.await.context("network supervisor task panicked")?)
    } else {
        None
    };
    let simulation_result = simulation.shutdown();
    application_result?;
    if let Some(result) = network_result {
        result.context("network supervisor failed")?;
    }
    let exit = simulation_result.context("simulation host shutdown failed")?;
    tracing::info!(
        target: "blackflower_server",
        event_name = "simulation_stopped",
        completed_ticks = exit.completed_ticks,
        "authoritative simulation stopped",
    );
    observability.report_health();
    Ok(())
}

fn validate_arguments(arguments: &Arguments) -> Result<()> {
    if arguments
        .listen_address
        .is_some_and(|address| !address.ip().is_loopback())
    {
        bail!("--listen-address must be loopback for the local identity authority");
    }
    if arguments.foreground && (!std::io::stdin().is_terminal() || !std::io::stdout().is_terminal())
    {
        bail!("--foreground requires an interactive terminal");
    }
    Ok(())
}

async fn run_application(
    arguments: &Arguments,
    config: &ObservabilityConfig,
    metrics_address: Option<SocketAddr>,
    observability: &mut ObservabilityGuard,
    shutdown_requested: Arc<AtomicBool>,
) -> Result<()> {
    if arguments.foreground {
        run_foreground(config, metrics_address, observability, shutdown_requested).await
    } else {
        shutdown_signal().await
    }
}

async fn run_foreground(
    config: &ObservabilityConfig,
    metrics_address: Option<SocketAddr>,
    observability: &mut ObservabilityGuard,
    shutdown_requested: Arc<AtomicBool>,
) -> Result<()> {
    let metrics_address = metrics_address.context("foreground metrics endpoint is disabled")?;
    let (log_receiver, log_control) = observability
        .take_foreground_logs()
        .context("foreground log capture is disabled")?;
    let foreground_lifetime = Arc::clone(&shutdown_requested);
    let foreground_config = ForegroundConfig {
        service_name: config.service_name(),
        service_version: env!("CARGO_PKG_VERSION"),
        metrics_address,
        log_receiver,
        log_control,
        shutdown_requested: Arc::clone(&shutdown_requested),
    };
    let mut foreground_task = tokio::task::spawn_blocking(move || {
        let result = foreground::run(foreground_config);
        foreground_lifetime.store(true, Ordering::Release);
        result
    });

    tokio::select! {
        result = &mut foreground_task => result
            .context("foreground task panicked")?
            .context("foreground mode failed"),
        signal_result = shutdown_signal() => {
            shutdown_requested.store(true, Ordering::Release);
            let foreground_result = foreground_task
                .await
                .context("foreground task panicked")?;
            signal_result?;
            foreground_result.context("foreground mode failed")
        }
    }
}

fn network_runtime(
    arguments: &Arguments,
    simulation: SimulationStatus,
) -> Result<Option<ServerNetworkRuntime>> {
    let Some(bind_address) = arguments.listen_address else {
        return Ok(None);
    };
    let tls = server_tls(arguments)?;
    let contract = compatibility_contract();
    let content = content_manifest(arguments)?;
    let endpoint = QuicServer::bind(ServerEndpointConfig {
        bind_address,
        tls,
        admission_limits: local_admission_limits()?,
    })
    .context("QUIC server bind failed")?;
    let authority = LoopbackSessionAuthority::new(contract);
    let network = DedicatedServerNetwork::new(
        endpoint,
        authority,
        contract,
        content,
        BudgetTier::Preferred,
        Duration::ZERO,
    );
    Ok(Some(ServerNetworkRuntime::new(network, simulation)))
}

fn server_tls(arguments: &Arguments) -> Result<ServerTlsConfig> {
    let certificate_path =
        required_argument(arguments.tls_certificate.as_ref(), "--tls-certificate")?;
    let certificate_chain = CertificateDer::pem_file_iter(certificate_path)
        .with_context(|| {
            format!(
                "failed to open TLS certificate chain {}",
                certificate_path.display()
            )
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed to decode TLS certificate chain")?;
    if certificate_chain.is_empty() {
        bail!("TLS certificate chain is empty");
    }
    let private_key_path =
        required_argument(arguments.tls_private_key.as_ref(), "--tls-private-key")?;
    let private_key = PrivateKeyDer::from_pem_file(private_key_path).with_context(|| {
        format!(
            "failed to decode TLS private key {}",
            private_key_path.display()
        )
    })?;
    Ok(ServerTlsConfig {
        certificate_chain,
        private_key,
    })
}

const fn compatibility_contract() -> CompatibilityContract {
    CompatibilityContract {
        protocol_revision: ProtocolRevision::V1,
    }
}

fn content_manifest(arguments: &Arguments) -> Result<ContentManifest> {
    let directory = required_argument(
        arguments.asset_package_directory.as_ref(),
        "--asset-package-directory",
    )?;
    let trust_store = load_asset_trust_store(&arguments.asset_trust_keys)?;
    let assets = AssetStore::open_dir(directory, &trust_store).with_context(|| {
        format!(
            "failed to validate asset packages in {}",
            directory.display()
        )
    })?;
    Ok(ContentManifest {
        map_id: required_argument(arguments.map_id.as_ref(), "--map-id")?.clone(),
        required_content_set_id: RequiredContentSetId::from_bytes(
            *assets.asset_set_hash().as_bytes(),
        ),
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

fn local_admission_limits() -> Result<AdmissionLimits> {
    Ok(AdmissionLimits {
        attempts_per_window: NonZeroU32::new(32).context("attempt limit must be non-zero")?,
        window: Duration::from_secs(1),
        pending_per_origin: NonZeroUsize::new(4)
            .context("per-origin pending limit must be non-zero")?,
        pending_global: NonZeroUsize::new(16).context("pending limit must be non-zero")?,
        connections_global: NonZeroUsize::new(16).context("connection limit must be non-zero")?,
    })
}

fn required_argument<'a, T>(value: Option<&'a T>, name: &str) -> Result<&'a T> {
    value.with_context(|| format!("{name} is required with --listen-address"))
}

async fn shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .context("failed to install SIGTERM handler")?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.context("failed to wait for SIGINT")
            }
            signal = terminate.recv() => {
                signal.context("SIGTERM signal stream closed")
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .context("failed to wait for shutdown signal")
    }
}

#[cfg(test)]
#[path = "../tests/unit/arguments.rs"]
mod tests;
