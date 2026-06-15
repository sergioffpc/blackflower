//! Server-side QUIC endpoint.
//!
//! Spawns a tokio runtime on a dedicated background thread, binds a QUIC
//! endpoint, and accepts incoming connections. Each connection, after a
//! `Subscribe`, receives every snapshot the tick thread produces as a QUIC
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
use crossbeam_channel::TrySendError;
use quinn::{Endpoint, ServerConfig};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

use crate::{
    cert::generate_self_signed_cert,
    connection::{Connection, ConnectionId, Connections},
    delay::DelayConfig,
};

const SNAPSHOT_QUEUE_CAPACITY: usize = 8;

struct Addressed<M>(ConnectionId, M);

/// Handle to a running server endpoint.
///
/// The server runs on a dedicated background thread that owns a tokio runtime.
/// When this handle is dropped, the thread is signaled to shut down and joined.
pub struct ServerHandle<C, S, R, E> {
    command_rx: crossbeam_channel::Receiver<(ConnectionId, C)>,
    snapshot_tx: crossbeam_channel::Sender<Addressed<S>>,
    request_rx: crossbeam_channel::Receiver<(ConnectionId, R)>,
    event_tx: crossbeam_channel::Sender<Addressed<E>>,
    disconnect_rx: crossbeam_channel::Receiver<ConnectionId>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl<C, S, R, E> ServerHandle<C, S, R, E> {
    pub fn try_send_snapshot_to(&self, client_id: ConnectionId, snapshot: S) {
        match self.snapshot_tx.try_send(Addressed(client_id, snapshot)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                warn!(client = %client_id, "snapshot queue full; dropping");
            }
            Err(TrySendError::Disconnected(_)) => debug!("snapshot channel disconnected"),
        }
    }

    pub fn try_recv_commands(&self) -> impl Iterator<Item = (ConnectionId, C)> + '_ {
        self.command_rx.try_iter()
    }

    pub fn try_recv_requests(&self) -> impl Iterator<Item = (ConnectionId, R)> + '_ {
        self.request_rx.try_iter()
    }

    /// Drain clients that have disconnected since the last call.
    ///
    /// Originated by the transport itself (not the peer): when a QUIC
    /// connection closes, the network emits the corresponding
    /// [`ConnectionId`] here so the application can release that client's
    /// resources (e.g. despawn its avatar).
    pub fn try_recv_disconnects(&self) -> impl Iterator<Item = ConnectionId> + '_ {
        self.disconnect_rx.try_iter()
    }

    pub fn try_send_event_to(&self, client_id: ConnectionId, event: E) {
        match self.event_tx.try_send(Addressed(client_id, event)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => warn!(client = %client_id, "event queue full; dropping"),
            Err(TrySendError::Disconnected(_)) => debug!("event channel disconnected"),
        }
    }
}

impl<C, S, R, E> Drop for ServerHandle<C, S, R, E> {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            tx.send(()).ok();
        }
        if let Some(handle) = self.join_handle.take()
            && let Err(e) = handle.join()
        {
            error!(error = ?e, "network server thread");
        }
    }
}

pub fn start<C, S, R, E>(
    bind_addr: SocketAddr,
    delay: DelayConfig,
) -> anyhow::Result<ServerHandle<C, S, R, E>>
where
    C: Send + Sync + DeserializeOwned + 'static,
    S: Clone + Send + Sync + Serialize + 'static,
    R: Send + DeserializeOwned + 'static,
    E: Clone + Send + Sync + Serialize + 'static,
{
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

    let (command_tx, command_rx) = crossbeam_channel::unbounded();
    let (snapshot_tx, snapshot_rx) = crossbeam_channel::bounded(SNAPSHOT_QUEUE_CAPACITY);
    let (request_tx, request_rx) = crossbeam_channel::unbounded();
    let (event_tx, event_rx) = crossbeam_channel::unbounded::<Addressed<E>>();
    let (disconnect_tx, disconnect_rx) = crossbeam_channel::unbounded::<ConnectionId>();
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

                        accept_loop(
                            ep,
                            delay,
                            ServerChannels {
                                incoming: Arc::new(IncomingChannels {
                                    command_tx,
                                    request_tx,
                                }),
                                outgoing: OutgoingChannels {
                                    snapshot_rx,
                                    event_rx,
                                },
                                disconnect_tx,
                            },
                            shutdown_rx,
                        )
                        .await;
                    }
                    Err(e) => {
                        error!(error = %e, "server endpoint");
                    }
                }
            });
        })
        .context("spawning network server thread")?;

    Ok(ServerHandle {
        command_rx,
        snapshot_tx,
        request_rx,
        event_tx,
        disconnect_rx,
        shutdown_tx: Some(shutdown_tx),
        join_handle: Some(join_handle),
    })
}

struct ServerChannels<C, S, R, E> {
    incoming: Arc<IncomingChannels<C, R>>,
    outgoing: OutgoingChannels<S, E>,
    disconnect_tx: crossbeam_channel::Sender<ConnectionId>,
}

struct OutgoingChannels<S, E> {
    snapshot_rx: crossbeam_channel::Receiver<Addressed<S>>,
    event_rx: crossbeam_channel::Receiver<Addressed<E>>,
}

struct IncomingChannels<C, R> {
    command_tx: crossbeam_channel::Sender<(ConnectionId, C)>,
    request_tx: crossbeam_channel::Sender<(ConnectionId, R)>,
}

async fn accept_loop<C, S, R, E>(
    endpoint: Endpoint,
    delay: DelayConfig,
    channels: ServerChannels<C, S, R, E>,
    mut shutdown_rx: oneshot::Receiver<()>,
) where
    C: Send + Sync + DeserializeOwned + 'static,
    S: Clone + Send + Sync + Serialize + 'static,
    R: Send + DeserializeOwned + 'static,
    E: Clone + Send + Sync + Serialize + 'static,
{
    let connections = Arc::new(Connections::new());

    let snapshot_rx = channels.outgoing.snapshot_rx;
    let conn_for_snapshots = Arc::clone(&connections);
    tokio::task::spawn_blocking(move || {
        while let Ok(Addressed(client_id, snapshot)) = snapshot_rx.recv() {
            conn_for_snapshots.try_send_snapshot_to(client_id, snapshot);
        }
        debug!("exiting snapshot dispatcher");
    });

    let event_rx = channels.outgoing.event_rx;
    let conn_for_events = Arc::clone(&connections);
    tokio::task::spawn_blocking(move || {
        while let Ok(Addressed(client_id, event)) = event_rx.recv() {
            conn_for_events.try_send_event_to(client_id, event);
        }
        debug!("exiting event dispatcher");
    });

    tokio::select! {
        // Prioritize shutdown handling over incoming connections.
        // With `biased`, Tokio evaluates branches top-to-bottom, so if both
        // shutdown and accept are ready at the same time, shutdown wins.
        // This avoids accepting new connections while shutting down.
        biased;

        _ = &mut shutdown_rx => {
            info!("shutdown signal received; closing endpoint");
            endpoint.close(0_u32.into(), b"shut down");
        }
        () = incoming_loop(&endpoint, delay, connections, channels.incoming, channels.disconnect_tx) => {}
    }

    endpoint.wait_idle().await;
}

async fn incoming_loop<C, S, R, E>(
    endpoint: &Endpoint,
    delay: DelayConfig,
    connections: Arc<Connections<S, E>>,
    channels: Arc<IncomingChannels<C, R>>,
    disconnect_tx: crossbeam_channel::Sender<ConnectionId>,
) where
    C: Send + Sync + DeserializeOwned + 'static,
    S: Clone + Send + Sync + Serialize + 'static,
    R: Send + DeserializeOwned + 'static,
    E: Clone + Send + Sync + Serialize + 'static,
{
    loop {
        let Some(incoming) = endpoint.accept().await else {
            info!("endpoint closed; exiting accept loop");
            break;
        };
        let connections = Arc::clone(&connections);
        let channels = channels.clone();
        let disconnect_tx = disconnect_tx.clone();
        tokio::spawn(async move {
            match Connection::new(incoming, delay).await {
                Ok(mut connection) => {
                    let client_id = connections.insert(&connection).await;
                    connection
                        .connection_loop(
                            client_id,
                            channels.command_tx.clone(),
                            channels.request_tx.clone(),
                        )
                        .await;
                    // Remove routing entry before notifying the game loop so
                    // that no snapshot dispatches hit a dead channel between
                    // the disconnect signal and the map cleanup.
                    connections.remove(&client_id).await;
                    disconnect_tx.send(client_id).ok();
                    // Give QUIC a bounded window to drain cleanly; unresponsive
                    // peers won't hold the task open indefinitely.
                    tokio::time::timeout(
                        std::time::Duration::from_secs(5),
                        connection.wait_for_close(),
                    )
                    .await
                    .ok();
                }
                Err(e) => {
                    error!(error = %e, "connection closed");
                }
            }
        });
    }
}
