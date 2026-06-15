use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use anyhow::Context;
use hashbrown::HashMap;
use quinn::{ConnectionError, Incoming, RecvStream, SendStream};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::sync::{
    Mutex,
    mpsc::{self, error::TrySendError},
};
use tracing::{debug, error, info, warn};

use crate::{
    decode, decode_framed,
    delay::{DelayConfig, DelayQueue},
    encode, encode_framed,
};

const PER_CLIENT_SNAPSHOT_CAPACITY: usize = 3;
const PER_CLIENT_EVENT_CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId(u64);

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct Connection<S, E> {
    conn: quinn::Connection,
    delay: DelayConfig,

    send: SendStream,
    recv: RecvStream,

    pub(crate) snapshot_tx: mpsc::Sender<Arc<S>>,
    snapshot_rx: mpsc::Receiver<Arc<S>>,

    pub(crate) event_tx: mpsc::Sender<E>,
    event_rx: mpsc::Receiver<E>,
}

impl<S, E> Connection<S, E> {
    pub async fn new(incoming: Incoming, delay: DelayConfig) -> anyhow::Result<Self> {
        let conn = incoming.await.context("connection handshake failed")?;
        let remote_addr = conn.remote_address();
        info!(remote = %remote_addr, "client connected");

        let (send, recv) = conn
            .accept_bi()
            .await
            .context("accepting bidirectional stream")?;

        let (snapshot_tx, snapshot_rx) = mpsc::channel::<Arc<S>>(PER_CLIENT_SNAPSHOT_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel::<E>(PER_CLIENT_EVENT_CAPACITY);

        Ok(Self {
            conn,
            delay,
            send,
            recv,
            snapshot_tx,
            snapshot_rx,
            event_tx,
            event_rx,
        })
    }

    pub async fn connection_loop<C, R>(
        &mut self,
        client_id: ConnectionId,
        command_tx: crossbeam_channel::Sender<(ConnectionId, C)>,
        request_tx: crossbeam_channel::Sender<(ConnectionId, R)>,
    ) where
        C: Send + Sync + DeserializeOwned,
        E: Serialize + Send + Sync,
        R: DeserializeOwned,
        S: Serialize + Send + Sync,
    {
        // Destructure into disjoint field borrows so the borrow checker can
        // see that the four concurrent futures touch independent state.
        let Self {
            conn,
            delay,
            send,
            recv,
            snapshot_rx,
            event_rx,
            ..
        } = self;

        tokio::select! {
            () = recv_commands_loop(conn, *delay, client_id, &command_tx) => {}
            () = send_snapshots_loop(conn, snapshot_rx) => {}
            () = recv_requests_loop(recv, client_id, &request_tx) => {}
            () = send_events_loop(send, event_rx) => {}
        }
    }

    pub async fn wait_for_close(&self) {
        self.conn.closed().await;
    }
}

async fn send_snapshots_loop<S>(
    connection: &quinn::Connection,
    snapshot_rx: &mut mpsc::Receiver<Arc<S>>,
) where
    S: Serialize + Send + Sync,
{
    while let Some(snapshot) = snapshot_rx.recv().await {
        let data = match encode(&*snapshot) {
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

async fn recv_commands_loop<C>(
    connection: &quinn::Connection,
    delay: DelayConfig,
    client_id: ConnectionId,
    command_tx: &crossbeam_channel::Sender<(ConnectionId, C)>,
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

async fn send_events_loop<E>(send: &mut SendStream, event_rx: &mut mpsc::Receiver<E>)
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

async fn recv_requests_loop<R>(
    recv: &mut RecvStream,
    client_id: ConnectionId,
    request_tx: &crossbeam_channel::Sender<(ConnectionId, R)>,
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

struct ConnectionEntry<S, E> {
    snapshot_tx: mpsc::Sender<Arc<S>>,
    event_tx: mpsc::Sender<E>,
}

pub struct Connections<S, E> {
    inner: Mutex<HashMap<ConnectionId, ConnectionEntry<S, E>>>,
    client_id_counter: AtomicU64,
}

impl<S, E> Connections<S, E> {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            client_id_counter: AtomicU64::new(1),
        }
    }

    /// Register a connection's outbound senders and return its assigned [`ConnectionId`].
    pub async fn insert(&self, connection: &Connection<S, E>) -> ConnectionId {
        let client_id = ConnectionId(self.client_id_counter.fetch_add(1, Ordering::Relaxed));
        self.inner.lock().await.insert(
            client_id,
            ConnectionEntry {
                snapshot_tx: connection.snapshot_tx.clone(),
                event_tx: connection.event_tx.clone(),
            },
        );
        client_id
    }

    pub async fn remove(&self, client_id: &ConnectionId) {
        self.inner.lock().await.remove(client_id);
    }

    pub fn try_send_snapshot_to(&self, client_id: ConnectionId, snapshot: S) {
        // Clone the sender under the lock, then release before allocating the
        // Arc and calling try_send — minimises critical-section duration.
        let sender = self.inner.blocking_lock().get(&client_id).map(|e| e.snapshot_tx.clone());
        match sender {
            Some(tx) => match tx.try_send(Arc::new(snapshot)) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    debug!(client = %client_id, "client snapshot queue full; dropping");
                }
                Err(TrySendError::Closed(_)) => {
                    debug!(client = %client_id, "client snapshot channel closed; dropping");
                }
            },
            None => debug!(client = %client_id, "snapshot for unknown client; dropping"),
        }
    }

    pub fn try_send_event_to(&self, client_id: ConnectionId, event: E) {
        let sender = self.inner.blocking_lock().get(&client_id).map(|e| e.event_tx.clone());
        match sender {
            Some(tx) => match tx.try_send(event) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    warn!(client = %client_id, "client event queue full; dropping");
                }
                Err(TrySendError::Closed(_)) => {
                    debug!(client = %client_id, "client event channel closed; dropping");
                }
            },
            None => debug!(client = %client_id, "event for unknown client; dropping"),
        }
    }
}
