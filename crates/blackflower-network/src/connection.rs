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

const PER_CONN_SNAPSHOT_CAPACITY: usize = 3;
const PER_CONN_EVENT_CAPACITY: usize = 32;

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

impl<S, E> Connection<S, E>
where
    S: Send + Sync,
    E: Send + Sync,
{
    pub async fn new(incoming: Incoming, delay: DelayConfig) -> anyhow::Result<Self> {
        let conn = incoming.await.context("connection handshake failed")?;
        let remote_addr = conn.remote_address();
        info!(remote = %remote_addr, "connection connected");

        let (send, recv) = conn
            .accept_bi()
            .await
            .context("accepting bidirectional stream")?;

        let (snapshot_tx, snapshot_rx) = mpsc::channel::<Arc<S>>(PER_CONN_SNAPSHOT_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel::<E>(PER_CONN_EVENT_CAPACITY);

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
        conn_id: ConnectionId,
        command_tx: crossbeam_channel::Sender<(ConnectionId, C)>,
        request_tx: crossbeam_channel::Sender<(ConnectionId, R)>,
    ) where
        C: Send + Sync + DeserializeOwned,
        E: Serialize + Send + Sync,
        R: DeserializeOwned,
        S: Serialize + Send + Sync,
    {
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
            () = recv_commands_loop(conn, *delay, conn_id, &command_tx) => {}
            () = send_snapshots_loop(conn, snapshot_rx) => {}
            () = recv_requests_loop(recv, conn_id, &request_tx) => {}
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
    conn_id: ConnectionId,
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
                    if command_tx.try_send((conn_id, command)).is_err() {
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
                        info!(connection = %conn_id, "connection closed connection");
                        break;
                    }
                    Err(e) => {
                        warn!(error = %e, "connection lost");
                        break;
                    }
                };

                match decode::<C>(&bytes) {
                    Ok(command) => {
                        if delay.is_enabled() {
                            queue.push(command);
                        } else if command_tx.try_send((conn_id, command)).is_err() {
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
    conn_id: ConnectionId,
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
                info!(connection = %conn_id, "control stream closed by connection");
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

            if let Err(e) = request_tx.try_send((conn_id, request)) {
                debug!(error = %e, "request receiver dropped; exiting");
                return;
            }
        }
    }
}

struct ConnectionHandle<S, E> {
    snapshot_tx: mpsc::Sender<Arc<S>>,
    event_tx: mpsc::Sender<E>,
}

type ConnectionRegistry<S, E> = HashMap<ConnectionId, ConnectionHandle<S, E>>;

pub struct Connections<S, E> {
    registry: Mutex<ConnectionRegistry<S, E>>,
    conn_id_counter: AtomicU64,
}

impl<S, E> Connections<S, E>
where
    S: Send + Sync,
    E: Send + Sync,
{
    pub fn new() -> Self {
        Self {
            registry: Mutex::new(HashMap::new()),
            conn_id_counter: AtomicU64::new(1),
        }
    }

    pub async fn insert(&self, connection: &Connection<S, E>) -> ConnectionId {
        let conn_id = ConnectionId(self.conn_id_counter.fetch_add(1, Ordering::Relaxed));
        self.registry.lock().await.insert(
            conn_id,
            ConnectionHandle {
                snapshot_tx: connection.snapshot_tx.clone(),
                event_tx: connection.event_tx.clone(),
            },
        );
        conn_id
    }

    pub async fn remove(&self, conn_id: &ConnectionId) {
        self.registry.lock().await.remove(conn_id);
    }

    pub fn try_send_snapshot_to(&self, conn_id: ConnectionId, snapshot: S) {
        let sender = self
            .registry
            .blocking_lock()
            .get(&conn_id)
            .map(|e| e.snapshot_tx.clone());
        if let Some(tx) = sender {
            match tx.try_send(Arc::new(snapshot)) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    debug!(connection = %conn_id, "connection snapshot queue full; dropping");
                }
                Err(TrySendError::Closed(_)) => {
                    debug!(connection = %conn_id, "connection snapshot channel closed; dropping");
                }
            }
        } else {
            debug!(connection = %conn_id, "snapshot for unknown connection; dropping");
        }
    }

    pub fn try_send_event_to(&self, conn_id: ConnectionId, event: E) {
        let sender = self
            .registry
            .blocking_lock()
            .get(&conn_id)
            .map(|e| e.event_tx.clone());
        if let Some(tx) = sender {
            match tx.try_send(event) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    warn!(connection = %conn_id, "connection event queue full; dropping");
                }
                Err(TrySendError::Closed(_)) => {
                    debug!(connection = %conn_id, "connection event channel closed; dropping");
                }
            }
        } else {
            debug!(connection = %conn_id, "event for unknown connection; dropping");
        }
    }
}

impl<S, E> Default for Connections<S, E>
where
    S: Send + Sync,
    E: Send + Sync,
{
    fn default() -> Self {
        Self::new()
    }
}
