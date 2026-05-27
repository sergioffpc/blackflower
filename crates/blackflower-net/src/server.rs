//! Server-side QUIC endpoint.
//!
//! Spawns a tokio runtime on a dedicated background thread, binds a QUIC
//! endpoint, and accepts incoming connections. Each connection, after a
//! `Hello`, receives every snapshot the tick thread produces as a QUIC
//! datagram.
//!
//! ## Broadcast architecture
//!
//! Three layers:
//!
//! 1. **Tick thread** pushes `Snapshot` into a bounded `crossbeam` channel
//!    via [`ServerHandle::send_snapshot`].
//!
//! 2. **Dispatcher task** drains the crossbeam channel, wraps each snapshot
//!    in an `Arc`, and fans it out to every active connection's own per-
//!    client tokio channel.
//!
//! 3. **Per-connection task** pops snapshots from its own channel, encodes
//!    them, and sends them as datagrams. If a client is slow, its channel
//!    fills and snapshots for that client are dropped — other clients are
//!    unaffected.

use std::{net::SocketAddr, sync::Arc, thread::JoinHandle};

use anyhow::Context;
use blackflower_core::ecs::Snapshot;
use crossbeam_channel::TrySendError;
use quinn::{Endpoint, Incoming, ServerConfig};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use crate::{
    cert::generate_self_signed_cert,
    messages::{ClientToServer, ServerToClient, decode, encode},
};

/// Bounded capacity of the tick → network snapshot queue.
const SNAPSHOT_QUEUE_CAPACITY: usize = 8;

/// Bounded capacity of the per-client snapshot queue.
///
/// Small (3 ticks ≈ 50ms at 60 Hz) so that a slow client falls behind
/// quickly and we drop snapshots rather than buffering long.
const PER_CLIENT_QUEUE_CAPACITY: usize = 3;

/// Handle to a running server endpoint.
///
/// The server runs on a dedicated background thread that owns a tokio runtime.
/// When this handle is dropped, the thread is signaled to shut down and joined.
pub struct ServerHandle {
    snapshot_tx: crossbeam_channel::Sender<Snapshot>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl ServerHandle {
    /// Enqueue a snapshot for transmission to all connected clients.
    ///
    /// Drops the snapshot (with a warning) if the queue is full. A dropped
    /// snapshot is immediately superseded by the next one, so this is
    /// preferable to blocking the tick thread.
    pub fn send_snapshot(&self, snapshot: Snapshot) {
        match self.snapshot_tx.try_send(snapshot) {
            Ok(()) => (),
            Err(TrySendError::Full(_)) => warn!("snapshot queue full; dropping snapshot"),
            Err(TrySendError::Disconnected(_)) => debug!("snapshot channel disconnected"),
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            tx.send(()).ok();
        }
        if let Some(handle) = self.join_handle.take()
            && let Err(err) = handle.join()
        {
            error!(error = ?err, "network server thread");
        }
    }
}

pub fn start(bind_addr: SocketAddr) -> anyhow::Result<ServerHandle> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let (certs, key) = generate_self_signed_cert().context("self-signed cert")?;
    let mut rustls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("rustls server config")?;
    rustls_config.alpn_protocols = vec![b"blackflower/0".to_vec()];

    let quic_config = quinn::crypto::rustls::QuicServerConfig::try_from(rustls_config)
        .context("converting rustls to QUIC server config")?;
    let server_config = ServerConfig::with_crypto(Arc::new(quic_config));

    let (snapshot_tx, snapshot_rx) = crossbeam_channel::bounded(SNAPSHOT_QUEUE_CAPACITY);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let join_handle = std::thread::Builder::new()
        .name("blackflower-net::server".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    error!(error = %e, "building server async runtime");
                    return;
                }
            };

            runtime.block_on(async move {
                match Endpoint::server(server_config, bind_addr) {
                    Ok(ep) => {
                        let local_addr = ep.local_addr().unwrap_or(bind_addr);
                        info!(local = %local_addr, "listening");

                        accept_loop(ep, snapshot_rx, shutdown_rx).await;
                    }
                    Err(e) => {
                        error!(error = %e, "server endpoint");
                    }
                }
            });
        })
        .context("spawning network server thread")?;

    Ok(ServerHandle {
        snapshot_tx,
        shutdown_tx: Some(shutdown_tx),
        join_handle: Some(join_handle),
    })
}

/// One subscriber registered with the dispatcher.
///
/// The dispatcher pushes `Arc<Snapshot>` here; the connection task pops.
type ConnectionSender = mpsc::Sender<Arc<Snapshot>>;

/// Registry of active subscribers. Shared between the dispatcher and the
/// accept loop. A `Mutex` is acceptable because contention is negligible:
/// the dispatcher touches it only on subscribe/unsubscribe.
type Subscribers = Arc<Mutex<Vec<ConnectionSender>>>;

async fn accept_loop(
    endpoint: Endpoint,
    snapshot_rx: crossbeam_channel::Receiver<Snapshot>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let subscribers: Subscribers = Arc::new(Mutex::new(Vec::new()));
    let subscribers_clone = Arc::clone(&subscribers);
    tokio::task::spawn_blocking(move || {
        while let Ok(snapshot) = snapshot_rx.recv() {
            let snapshot = Arc::new(snapshot);
            let subscribers_guard = subscribers_clone.blocking_lock();
            for sender in subscribers_guard.iter() {
                #[allow(clippy::excessive_nesting)]
                if let Err(mpsc::error::TrySendError::Full(_)) =
                    sender.try_send(Arc::clone(&snapshot))
                {
                    debug!("client queue full; dropping snapshot");
                }
            }
        }
        debug!("exiting dispatcher");
    });

    loop {
        tokio::select! {
            // Prioritize shutdown handling over incoming connections.
            // With `biased`, Tokio evaluates branches top-to-bottom, so if both
            // shutdown and accept are ready at the same time, shutdown wins.
            // This avoids accepting new connections while shutting down.
            biased;

            _ = &mut shutdown_rx => {
                info!("shutdown signal received; closing endpoint");
                endpoint.close(0_u32.into(), b"server shutted down");
                break;
            }

            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else {
                    info!("endpoint closed; exiting accept loop");
                    break;
                };
                let subs = Arc::clone(&subscribers);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(incoming, subs).await {
                        error!(error = %e, "connection closed");
                    }
                });
            }
        }
    }

    endpoint.wait_idle().await;
}

async fn handle_connection(incoming: Incoming, subscribers: Subscribers) -> anyhow::Result<()> {
    let connection = incoming.await.context("connection handshake failed")?;
    let remote_addr = connection.remote_address();
    info!(remote = %remote_addr, "client connected");

    let (mut send, mut recv) = connection
        .accept_bi()
        .await
        .context("accepting bidirectional stream")?;

    let buf = recv
        .read_to_end(1024)
        .await
        .context("reading subscription message")?;
    let _: ClientToServer = decode(&buf).context("decoding subscription message")?;
    send.finish().context("finishing subscription stream")?;
    info!(remote = %remote_addr, "client subscribed for snapshot");

    // Register a per-client snapshot channel.
    let (snapshot_tx, mut snapshot_rx) = mpsc::channel::<Arc<Snapshot>>(PER_CLIENT_QUEUE_CAPACITY);
    {
        let mut subscribers_guard = subscribers.lock().await;
        subscribers_guard.push(snapshot_tx);
    }

    // Drive the snapshot stream until the connection breaks or the channel
    // closes.
    while let Some(snapshot) = snapshot_rx.recv().await {
        let data = match encode(&ServerToClient::Snapshot((*snapshot).clone())) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!(error = %e, "encoding snapshot failed");
                continue;
            }
        };

        if let Err(e) = connection.send_datagram(data) {
            error!(error = %e, remote = %remote_addr, "sending datagram failed");
            break;
        }
    }

    // Wait for the client to close the connection cleanly.
    connection.closed().await;
    info!(remote = %remote_addr, "client disconnected");

    {
        let mut subscribers_guard = subscribers.lock().await;
        subscribers_guard.retain(|s| !s.is_closed());
    }

    Ok(())
}
