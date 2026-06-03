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
use quinn::{Connection, Endpoint, Incoming, ServerConfig};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use crate::{
    cert::generate_self_signed_cert,
    messages::{Control, decode, encode},
};

/// Bounded capacity of the tick → network snapshot queue.
const WORLD_QUEUE_CAPACITY: usize = 8;

/// Bounded capacity of the per-client snapshot queue.
///
/// Small (3 ticks ≈ 50ms at 60 Hz) so that a slow client falls behind
/// quickly and we drop snapshots rather than buffering long.
const PER_CLIENT_QUEUE_CAPACITY: usize = 3;

/// Handle to a running server endpoint.
///
/// The server runs on a dedicated background thread that owns a tokio runtime.
/// When this handle is dropped, the thread is signaled to shut down and joined.
pub struct ServerHandle<I, W> {
    input_rx: crossbeam_channel::Receiver<I>,
    world_tx: crossbeam_channel::Sender<W>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl<I, W> ServerHandle<I, W> {
    /// Enqueue a snapshot for transmission to all connected clients.
    ///
    /// Drops the snapshot (with a warning) if the queue is full. A dropped
    /// snapshot is immediately superseded by the next one, so this is
    /// preferable to blocking the tick thread.
    pub fn try_send_world_snapshot(&self, world: W) {
        match self.world_tx.try_send(world) {
            Ok(()) => (),
            Err(TrySendError::Full(_)) => warn!("world queue full; dropping snapshot"),
            Err(TrySendError::Disconnected(_)) => debug!("world channel disconnected"),
        }
    }

    pub fn try_recv_input_snapshots(&self) -> Box<[I]> {
        let mut inputs = vec![];
        while let Ok(input) = self.input_rx.try_recv() {
            inputs.push(input);
        }
        inputs.into_boxed_slice()
    }
}

impl<I, W> Drop for ServerHandle<I, W> {
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

pub fn start<I, W>(bind_addr: SocketAddr) -> anyhow::Result<ServerHandle<I, W>>
where
    I: Send + DeserializeOwned + 'static,
    W: Clone + Send + Sync + Serialize + 'static,
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

    let (input_tx, input_rx) = crossbeam_channel::unbounded();
    let (world_tx, world_rx) = crossbeam_channel::bounded(WORLD_QUEUE_CAPACITY);
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

                        accept_loop(ep, input_tx, world_rx, shutdown_rx).await;
                    }
                    Err(e) => {
                        error!(error = %e, "server endpoint");
                    }
                }
            });
        })
        .context("spawning network server thread")?;

    Ok(ServerHandle {
        input_rx,
        world_tx,
        shutdown_tx: Some(shutdown_tx),
        join_handle: Some(join_handle),
    })
}

/// One subscriber registered with the dispatcher.
struct ConnectionSender<W>(mpsc::Sender<Arc<W>>);

/// Registry of active subscribers. Shared between the dispatcher and the
/// accept loop. A `Mutex` is acceptable because contention is negligible:
/// the dispatcher touches it only on subscribe/unsubscribe.
struct Connections<W>(Mutex<Vec<ConnectionSender<W>>>);

async fn accept_loop<I, W>(
    endpoint: Endpoint,
    input_tx: crossbeam_channel::Sender<I>,
    world_rx: crossbeam_channel::Receiver<W>,
    mut shutdown_rx: oneshot::Receiver<()>,
) where
    I: Send + DeserializeOwned + 'static,
    W: Clone + Send + Sync + Serialize + 'static,
{
    let connections = Arc::new(Connections(Mutex::new(Vec::new())));

    let connections_clone = Arc::clone(&connections);
    tokio::task::spawn_blocking(move || {
        while let Ok(world) = world_rx.recv() {
            let world = Arc::new(world);
            let connections_guard = connections_clone.0.blocking_lock();
            for sender in connections_guard.iter() {
                #[allow(clippy::excessive_nesting)]
                if let Err(mpsc::error::TrySendError::Full(_)) =
                    sender.0.try_send(Arc::clone(&world))
                {
                    debug!("client queue full; dropping snapshot");
                }
            }
        }
        debug!("exiting dispatcher");
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
        () = incoming_loop(&endpoint, connections, input_tx) => {}
    }

    endpoint.wait_idle().await;
}

async fn incoming_loop<I, W>(
    endpoint: &Endpoint,
    connections: Arc<Connections<W>>,
    input_tx: crossbeam_channel::Sender<I>,
) where
    I: Send + DeserializeOwned + 'static,
    W: Clone + Send + Sync + Serialize + 'static,
{
    loop {
        let Some(incoming) = endpoint.accept().await else {
            info!("endpoint closed; exiting accept loop");
            break;
        };
        let connections = Arc::clone(&connections);
        let input_tx = input_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(incoming, connections, input_tx).await {
                error!(error = %e, "connection closed");
            }
        });
    }
}

async fn handle_connection<I, W>(
    incoming: Incoming,
    connections: Arc<Connections<W>>,
    input_tx: crossbeam_channel::Sender<I>,
) -> anyhow::Result<()>
where
    I: DeserializeOwned,
    W: Clone + Send + Sync + Serialize,
{
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
        .context("reading connection message")?;
    let Control::Connect = decode(&buf).context("decoding connection message")?;
    send.finish().context("finishing connection stream")?;

    // Register a per-client snapshot channel.
    let (world_tx, world_rx) = mpsc::channel::<Arc<W>>(PER_CLIENT_QUEUE_CAPACITY);
    {
        let mut connections = connections.0.lock().await;
        connections.push(ConnectionSender(world_tx));
    }
    info!(remote = %remote_addr, "client connected");

    tokio::select! {
        () = recv_input_snapshot(&connection, input_tx.clone()) => {}
        () = send_world_snapshot(&connection, world_rx) => {}
    }

    // Wait for the client to close the connection cleanly.
    connection.closed().await;
    info!(remote = %remote_addr, "client disconnected");

    {
        let mut connections = connections.0.lock().await;
        connections.retain(|s| !s.0.is_closed());
    }

    Ok(())
}

async fn send_world_snapshot<W>(connection: &Connection, mut world_rx: mpsc::Receiver<Arc<W>>)
where
    W: Clone + Send + Sync + Serialize,
{
    // Drive the snapshot stream until the connection breaks or the channel
    // closes.
    while let Some(world) = world_rx.recv().await {
        let data = match encode(&(*world).clone()) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!(error = %e, "encoding world snapshot failed");
                continue;
            }
        };

        if let Err(e) = connection.send_datagram(data) {
            error!(error = %e, remote = %connection.remote_address(), "sending datagram failed");
            break;
        }
    }
}

async fn recv_input_snapshot<I>(connection: &Connection, input_tx: crossbeam_channel::Sender<I>)
where
    I: DeserializeOwned,
{
    loop {
        let bytes = match connection.accept_uni().await {
            Ok(mut recv) => match recv.read_to_end(1024).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    warn!(error = %e, "reading stream failed");
                    continue;
                }
            },
            Err(e) => {
                error!(error = %e, "opening input stream");
                break;
            }
        };

        match decode::<I>(&bytes) {
            Ok(input) => {
                if let Err(e) = input_tx.try_send(input) {
                    error!(error = %e, "input receiver dropped; exiting receive loop");
                    break;
                }
            }
            Err(e) => {
                warn!(error = %e, "decoding packet failed");
            }
        }
    }
}
