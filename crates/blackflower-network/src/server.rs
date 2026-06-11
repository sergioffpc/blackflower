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

use std::{
    net::SocketAddr,
    sync::{Arc, atomic::AtomicU64},
    thread::JoinHandle,
};

use anyhow::Context;
use crossbeam_channel::TrySendError;
use hashbrown::HashMap;
use quinn::{
    Connection, ConnectionError, Endpoint, Incoming, RecvStream, SendStream, ServerConfig,
};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use crate::{
    ClientId,
    cert::generate_self_signed_cert,
    decode, decode_framed,
    delay::{DelayConfig, DelayQueue},
    encode, encode_framed,
};

const SNAPSHOT_QUEUE_CAPACITY: usize = 8;
const PER_CLIENT_SNAPSHOT_CAPACITY: usize = 3;
const PER_CLIENT_EVENT_CAPACITY: usize = 32;

/// Outgoing event addressed to a specific client.
struct AddressedEvent<Event>(ClientId, Event);

/// Handle to a running server endpoint.
///
/// The server runs on a dedicated background thread that owns a tokio runtime.
/// When this handle is dropped, the thread is signaled to shut down and joined.
pub struct ServerHandle<C, S, R, E> {
    command_rx: crossbeam_channel::Receiver<(ClientId, C)>,
    snapshot_tx: crossbeam_channel::Sender<S>,
    request_rx: crossbeam_channel::Receiver<(ClientId, R)>,
    event_tx: crossbeam_channel::Sender<AddressedEvent<E>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl<C, S, R, E> ServerHandle<C, S, R, E> {
    /// Enqueue a snapshot for transmission to all connected clients.
    ///
    /// Drops the snapshot (with a warning) if the queue is full. A dropped
    /// snapshot is immediately superseded by the next one, so this is
    /// preferable to blocking the tick thread.
    pub fn try_send_snapshot(&self, snapshot: S) {
        match self.snapshot_tx.try_send(snapshot) {
            Ok(()) => (),
            Err(TrySendError::Full(_)) => warn!("snapshot queue full; dropping snapshot"),
            Err(TrySendError::Disconnected(_)) => debug!("snapshot channel disconnected"),
        }
    }

    pub fn try_recv_commands(&self) -> impl Iterator<Item = (ClientId, C)> + '_ {
        self.command_rx.try_iter()
    }

    pub fn try_recv_requests(&self) -> impl Iterator<Item = (ClientId, R)> + '_ {
        self.request_rx.try_iter()
    }

    pub fn try_send_event_to(&self, client_id: ClientId, event: E) {
        match self.event_tx.try_send(AddressedEvent(client_id, event)) {
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
    let (event_tx, event_rx) = crossbeam_channel::unbounded::<AddressedEvent<E>>();
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
                            ServerChannelGroupDescriptor {
                                command_tx,
                                snapshot_rx,
                                request_tx,
                                event_rx,
                                shutdown_rx,
                            },
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
        shutdown_tx: Some(shutdown_tx),
        join_handle: Some(join_handle),
    })
}

/// Per-client outbound senders, owned by the dispatcher.
struct ClientChannels<S, E> {
    snapshot_tx: mpsc::Sender<Arc<S>>,
    event_tx: mpsc::Sender<E>,
}

#[allow(clippy::type_complexity)]
struct Connections<S, E> {
    inner: Mutex<HashMap<ClientId, ClientChannels<S, E>>>,
}

struct ServerChannelGroupDescriptor<C, S, R, E> {
    command_tx: crossbeam_channel::Sender<(ClientId, C)>,
    snapshot_rx: crossbeam_channel::Receiver<S>,
    request_tx: crossbeam_channel::Sender<(ClientId, R)>,
    event_rx: crossbeam_channel::Receiver<AddressedEvent<E>>,
    shutdown_rx: oneshot::Receiver<()>,
}

async fn accept_loop<C, S, R, E>(
    endpoint: Endpoint,
    delay: DelayConfig,
    mut desc: ServerChannelGroupDescriptor<C, S, R, E>,
) where
    C: Send + Sync + DeserializeOwned + 'static,
    S: Clone + Send + Sync + Serialize + 'static,
    R: Send + DeserializeOwned + 'static,
    E: Clone + Send + Sync + Serialize + 'static,
{
    let connections = Arc::new(Connections {
        inner: Mutex::new(HashMap::new()),
    });
    let client_id_counter = Arc::new(AtomicU64::new(1));

    let connections_clone = Arc::clone(&connections);
    tokio::task::spawn_blocking(move || {
        while let Ok(snapshot) = desc.snapshot_rx.recv() {
            let snapshot = Arc::new(snapshot);
            let connections_guard = connections_clone.inner.blocking_lock();
            for channels in connections_guard.values() {
                if let Err(mpsc::error::TrySendError::Full(_)) =
                    channels.snapshot_tx.try_send(Arc::clone(&snapshot))
                {
                    debug!("client queue full; dropping snapshot");
                }
            }
        }
        debug!("exiting dispatcher");
    });

    let conn_for_events = Arc::clone(&connections);
    tokio::task::spawn_blocking(move || {
        while let Ok(AddressedEvent(client_id, event)) = desc.event_rx.recv() {
            let guard = conn_for_events.inner.blocking_lock();
            if let Some(channels) = guard.get(&client_id) {
                if let Err(mpsc::error::TrySendError::Full(_)) = channels.event_tx.try_send(event) {
                    warn!(client = %client_id, "client event queue full; dropping");
                }
            } else {
                debug!(client = %client_id, "event for unknown client; dropping");
            }
        }
        debug!("exiting event dispatcher");
    });

    tokio::select! {
        // Prioritize shutdown handling over incoming connections.
        // With `biased`, Tokio evaluates branches top-to-bottom, so if both
        // shutdown and accept are ready at the same time, shutdown wins.
        // This avoids accepting new connections while shutting down.
        biased;

        _ = &mut desc.shutdown_rx => {
            info!("shutdown signal received; closing endpoint");
            endpoint.close(0_u32.into(), b"shut down");
        }
        () = incoming_loop(&endpoint, delay, connections, client_id_counter, desc.command_tx, desc.request_tx) => {}
    }

    endpoint.wait_idle().await;
}

async fn incoming_loop<C, S, R, E>(
    endpoint: &Endpoint,
    delay: DelayConfig,
    connections: Arc<Connections<S, E>>,
    client_id_counter: Arc<AtomicU64>,
    command_tx: crossbeam_channel::Sender<(ClientId, C)>,
    request_tx: crossbeam_channel::Sender<(ClientId, R)>,
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
        let client_id = ClientId::allocate(&client_id_counter);
        let command_tx = command_tx.clone();
        let request_tx = request_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(
                incoming,
                delay,
                client_id,
                connections,
                command_tx,
                request_tx,
            )
            .await
            {
                error!(error = %e, "connection closed");
            }
        });
    }
}

async fn handle_connection<C, S, R, E>(
    incoming: Incoming,
    delay: DelayConfig,
    client_id: ClientId,
    connections: Arc<Connections<S, E>>,
    command_tx: crossbeam_channel::Sender<(ClientId, C)>,
    request_tx: crossbeam_channel::Sender<(ClientId, R)>,
) -> anyhow::Result<()>
where
    C: Send + Sync + DeserializeOwned,
    S: Clone + Send + Sync + Serialize,
    R: DeserializeOwned,
    E: Clone + Send + Sync + Serialize,
{
    let connection = incoming.await.context("connection handshake failed")?;
    let remote_addr = connection.remote_address();
    info!(remote = %remote_addr, "client connected");

    let (send, recv) = connection
        .accept_bi()
        .await
        .context("accepting bidirectional stream")?;

    // Register snapshot+event channels for this client.
    let (snapshot_tx, snapshot_rx) = mpsc::channel::<Arc<S>>(PER_CLIENT_SNAPSHOT_CAPACITY);
    let (event_tx, event_rx) = mpsc::channel::<E>(PER_CLIENT_EVENT_CAPACITY);
    {
        let mut guard = connections.inner.lock().await;
        guard.insert(
            client_id,
            ClientChannels {
                snapshot_tx,
                event_tx,
            },
        );
    }

    tokio::select! {
        () = recv_commands(&connection, delay, client_id, command_tx) => {}
        () = send_snapshot(&connection, snapshot_rx) => {}
        () = recv_requests(recv, client_id, request_tx) => {}
        () = send_events(send, event_rx) => {}
    }

    // Wait for the client to close the connection cleanly.
    connection.closed().await;
    info!(remote = %remote_addr, "client disconnected");

    {
        let mut connections = connections.inner.lock().await;
        connections.remove(&client_id);
    }

    Ok(())
}

async fn send_snapshot<S>(connection: &Connection, mut snapshot_rx: mpsc::Receiver<Arc<S>>)
where
    S: Clone + Send + Sync + Serialize,
{
    // Drive the snapshot stream until the connection breaks or the channel
    // closes.
    while let Some(snapshot) = snapshot_rx.recv().await {
        let data = match encode(&(*snapshot).clone()) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!(error = %e, "encoding snapshot failed");
                continue;
            }
        };

        if let Err(e) = connection.send_datagram(data) {
            error!(error = %e, remote = %connection.remote_address(), "sending datagram failed");
            break;
        }
    }
}

async fn recv_commands<C>(
    connection: &Connection,
    delay: DelayConfig,
    client_id: ClientId,
    command_tx: crossbeam_channel::Sender<(ClientId, C)>,
) where
    C: Send + Sync + DeserializeOwned,
{
    let mut queue = DelayQueue::new(delay);

    loop {
        let deliver_tick = async {
            match queue.next_deadline() {
                Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
                None => std::future::pending::<()>().await,
            }
        };

        tokio::select! {
            biased;

            () = deliver_tick => {
                for command in queue.drain_ready(std::time::Instant::now()) {
                    if command_tx.try_send((client_id, command)).is_err() {
                        debug!("command receiver dropped; exiting");
                        return;
                    }
                }
            }

            result = connection.read_datagram() => {
                let bytes = match result {
                    Ok(bytes) => bytes,
                    Err(
                        ConnectionError::ApplicationClosed(_)
                        | ConnectionError::ConnectionClosed(_),
                    ) => {
                        info!(client = %client_id, "client closed connection");
                        break;
                    }
                    Err(e) => {
                        warn!(error = %e, "reading datagram failed");
                        continue;
                    }
                };

                match decode::<C>(&bytes) {
                    Ok(command) => {
                        if delay.is_enabled() {
                            queue.push(command);
                        } else if command_tx.try_send((client_id, command)).is_err() {
                            debug!("command receiver dropped; exiting");
                            break;
                        }
                    }
                    Err(e) => warn!(error = %e, "decoding command failed"),
                }
            }
        }
    }
}

async fn send_events<E>(mut send: SendStream, mut event_rx: mpsc::Receiver<E>)
where
    E: Serialize,
{
    while let Some(event) = event_rx.recv().await {
        match encode_framed::<E>(&event) {
            Ok(bytes) => {
                if let Err(e) = send.write_all(&bytes).await {
                    error!(error = %e, "writing event to control stream");
                    break;
                }
            }
            Err(e) => warn!(error = %e, "encoding event failed"),
        }
    }
    send.finish().ok();
}

async fn recv_requests<R>(
    mut recv: RecvStream,
    client_id: ClientId,
    request_tx: crossbeam_channel::Sender<(ClientId, R)>,
) where
    R: DeserializeOwned,
{
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 1024];

    loop {
        match recv.read(&mut chunk).await {
            Ok(Some(n)) => {
                buf.extend_from_slice(&chunk[..n]);
            }
            Ok(None) => {
                info!(client = %client_id, "control stream closed by client");
                break;
            }
            Err(e) => {
                warn!(error = %e, "reading control stream failed");
                break;
            }
        }

        loop {
            let (request, consumed) = match decode_framed::<R>(&mut buf) {
                Ok(Some((request, consumed))) => (request, consumed),
                Ok(None) => break,
                Err(e) => {
                    warn!(error = %e, "decoding request failed; dropping buffer");
                    buf.clear();
                    break;
                }
            };
            buf.drain(..consumed);

            if let Err(e) = request_tx.try_send((client_id, request)) {
                debug!(error = %e, "request receiver dropped; exiting");
                return;
            }
        }
    }
}
