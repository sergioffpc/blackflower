use std::collections::{BTreeMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use blackflower_networking::{
    FlowId, MAX_BOOTSTRAP_BYTES, MAX_CONTROL_MESSAGE_BYTES, MAX_CONTROL_QUEUE_BYTES,
    MAX_SNAPSHOT_CHUNKS, StateBootstrapHeader, VoiceStreamId, decode_datagram, decode_frame,
};
use bytes::Bytes;
use tokio::sync::mpsc as tokio_mpsc;

use crate::streams::read_control_frame;
use crate::{
    BootstrapTransfer, ClientConnection, QuicError, ServerConnection, SessionControlStream,
};

const HOST_EVENT_CAPACITY: usize = 128;
const CONTROL_CAPACITY: usize = 64;
const SNAPSHOT_CAPACITY: usize = 32;
const TIME_SYNC_CAPACITY: usize = 16;
const BOOTSTRAP_CAPACITY: usize = 1;
const VOICE_STREAM_CAPACITY: usize = 4;
const VOICE_PACKETS_PER_STREAM: usize = 3;
const DATAGRAM_POLL_INTERVAL: Duration = Duration::from_millis(1);
const PATH_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Event delivered synchronously from Tokio transport tasks to the game host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkEvent {
    /// One complete reliable session-control frame.
    SessionControl(Vec<u8>),
    /// One validated common-header application DATAGRAM.
    Datagram(Bytes),
    /// One complete uncompressed full-state bootstrap.
    Bootstrap(BootstrapTransfer),
    /// The peer's validated network path moved to a different remote address.
    PathChanged {
        /// Previously validated remote address.
        previous: SocketAddr,
        /// Newly validated remote address.
        current: SocketAddr,
    },
    /// Transport stopped; the session machine decides recovery or close.
    TransportStopped,
}

/// Synchronous bounded host handle for a low-level client endpoint.
///
/// This type contains no game-client, input-source, prediction, or harness API.
#[derive(Debug)]
pub struct ClientNetworkHandle {
    connection: ClientConnection,
    control: SharedControlSender,
    latest_input: Arc<Mutex<Option<Vec<u8>>>>,
    time_sync: tokio_mpsc::Sender<Vec<u8>>,
    voice: SharedVoiceQueue,
    events: mpsc::Receiver<NetworkEvent>,
}

/// Synchronous bounded host handle for a dedicated-server connection.
#[derive(Debug)]
pub struct ServerNetworkHandle {
    connection: ServerConnection,
    control: SharedControlSender,
    snapshots: tokio_mpsc::Sender<Vec<Vec<u8>>>,
    time_sync: tokio_mpsc::Sender<Vec<u8>>,
    voice: SharedVoiceQueue,
    bootstrap: tokio_mpsc::Sender<BootstrapTransfer>,
    bootstrap_pending: Arc<AtomicBool>,
    events: mpsc::Receiver<NetworkEvent>,
}

impl ClientConnection {
    /// Start Tokio I/O tasks and return the synchronous bounded client handle.
    pub async fn spawn_io(self) -> Result<ClientNetworkHandle, QuicError> {
        let control_stream = self.open_session_control().await?;
        let (control, control_receive) = control_channel();
        let latest_input = Arc::new(Mutex::new(None));
        let (time_sync, time_sync_receive) = tokio_mpsc::channel(TIME_SYNC_CAPACITY);
        let voice = SharedVoiceQueue::default();
        let (events_send, events) = mpsc::sync_channel(HOST_EVENT_CAPACITY);
        spawn_control_tasks(
            self.inner.clone(),
            control_stream,
            control_receive,
            Arc::clone(&control.queued_bytes),
            events_send.clone(),
        );
        spawn_datagram_receive(self.inner.clone(), events_send.clone());
        spawn_path_change_monitor(self.inner.clone(), events_send.clone());
        spawn_bootstrap_receive(self.clone(), events_send);
        spawn_client_datagram_send(
            self.clone(),
            Arc::clone(&latest_input),
            time_sync_receive,
            voice.clone(),
        );
        Ok(ClientNetworkHandle {
            connection: self,
            control,
            latest_input,
            time_sync,
            voice,
            events,
        })
    }
}

impl ServerConnection {
    /// Start Tokio I/O tasks and return the synchronous bounded server handle.
    pub async fn spawn_io(self) -> Result<ServerNetworkHandle, QuicError> {
        let control_stream = self.accept_session_control().await?;
        let (control, control_receive) = control_channel();
        let (snapshots, snapshot_receive) = tokio_mpsc::channel(SNAPSHOT_CAPACITY);
        let (time_sync, time_sync_receive) = tokio_mpsc::channel(TIME_SYNC_CAPACITY);
        let (bootstrap, bootstrap_receive) = tokio_mpsc::channel(BOOTSTRAP_CAPACITY);
        let bootstrap_pending = Arc::new(AtomicBool::new(false));
        let voice = SharedVoiceQueue::default();
        let (events_send, events) = mpsc::sync_channel(HOST_EVENT_CAPACITY);
        spawn_control_tasks(
            self.inner.clone(),
            control_stream,
            control_receive,
            Arc::clone(&control.queued_bytes),
            events_send.clone(),
        );
        spawn_datagram_receive(self.inner.clone(), events_send.clone());
        spawn_path_change_monitor(self.inner.clone(), events_send);
        spawn_server_datagram_send(
            self.clone(),
            snapshot_receive,
            time_sync_receive,
            voice.clone(),
        );
        spawn_bootstrap_send(
            self.clone(),
            bootstrap_receive,
            Arc::clone(&bootstrap_pending),
        );
        Ok(ServerNetworkHandle {
            connection: self,
            control,
            snapshots,
            time_sync,
            voice,
            bootstrap,
            bootstrap_pending,
            events,
        })
    }
}

impl ClientNetworkHandle {
    /// Queue a bounded reliable session-control frame.
    pub fn try_send_control(&self, frame: Vec<u8>) -> Result<(), QuicError> {
        validate_control(&frame)?;
        self.control.try_send(frame)
    }

    /// Replace any unsent input datagram with the newest exact datagram.
    pub fn set_latest_input(&self, datagram: Vec<u8>) -> Result<(), QuicError> {
        validate_flow(&datagram, FlowId::Input)?;
        self.connection.validate_datagram(&datagram)?;
        let mut latest = self
            .latest_input
            .lock()
            .map_err(|_poisoned| QuicError::QueueUnavailable)?;
        *latest = Some(datagram);
        Ok(())
    }

    /// Queue one bounded time-synchronization request datagram.
    pub fn try_send_time_sync(&self, datagram: Vec<u8>) -> Result<(), QuicError> {
        validate_flow(&datagram, FlowId::TimeSync)?;
        self.connection.validate_datagram(&datagram)?;
        try_send(&self.time_sync, datagram)
    }

    /// Queue at most three capture packets on each of at most four streams.
    pub fn try_send_voice(
        &self,
        stream: VoiceStreamId,
        datagram: Vec<u8>,
    ) -> Result<(), QuicError> {
        validate_flow(&datagram, FlowId::VoiceCapture)?;
        self.connection.validate_datagram(&datagram)?;
        self.voice.push(stream, datagram)
    }

    /// Poll one host event without blocking.
    pub fn try_receive(&self) -> Result<Option<NetworkEvent>, QuicError> {
        try_receive(&self.events)
    }

    /// Return cumulative UDP bytes reported by Quinn.
    #[must_use]
    pub fn udp_bytes(&self) -> crate::UdpByteStats {
        self.connection.udp_bytes()
    }
}

impl ServerNetworkHandle {
    /// Queue a bounded reliable session-control frame.
    pub fn try_send_control(&self, frame: Vec<u8>) -> Result<(), QuicError> {
        validate_control(&frame)?;
        self.control.try_send(frame)
    }

    /// Queue one one-to-four-chunk snapshot generation, retaining at most 32.
    pub fn try_send_snapshot_generation(&self, datagrams: Vec<Vec<u8>>) -> Result<(), QuicError> {
        if datagrams.is_empty() || datagrams.len() > MAX_SNAPSHOT_CHUNKS {
            return Err(QuicError::StreamRole);
        }
        for datagram in &datagrams {
            validate_flow(datagram, FlowId::SnapshotDelta)?;
            self.connection.validate_datagram(datagram)?;
        }
        try_send(&self.snapshots, datagrams)
    }

    /// Queue one bounded time-synchronization response datagram.
    pub fn try_send_time_sync(&self, datagram: Vec<u8>) -> Result<(), QuicError> {
        validate_flow(&datagram, FlowId::TimeSync)?;
        self.connection.validate_datagram(&datagram)?;
        try_send(&self.time_sync, datagram)
    }

    /// Queue at most three delivery packets on each of at most four streams.
    pub fn try_send_voice(
        &self,
        stream: VoiceStreamId,
        datagram: Vec<u8>,
    ) -> Result<(), QuicError> {
        validate_flow(&datagram, FlowId::VoiceDelivery)?;
        self.connection.validate_datagram(&datagram)?;
        self.voice.push(stream, datagram)
    }

    /// Queue the single active full-state bootstrap transfer.
    pub fn try_send_bootstrap(&self, transfer: BootstrapTransfer) -> Result<(), QuicError> {
        validate_bootstrap(&transfer.header, &transfer.body)?;
        self.bootstrap_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_pending| QuicError::QueueFull)?;
        if try_send(&self.bootstrap, transfer).is_err() {
            self.bootstrap_pending.store(false, Ordering::Release);
            return Err(QuicError::QueueFull);
        }
        Ok(())
    }

    /// Poll one host event without blocking.
    pub fn try_receive(&self) -> Result<Option<NetworkEvent>, QuicError> {
        try_receive(&self.events)
    }

    /// Return cumulative UDP bytes reported by Quinn.
    #[must_use]
    pub fn udp_bytes(&self) -> crate::UdpByteStats {
        self.connection.udp_bytes()
    }
}

impl Drop for ClientNetworkHandle {
    fn drop(&mut self) {
        self.connection.close(0, b"client network handle dropped");
    }
}

impl Drop for ServerNetworkHandle {
    fn drop(&mut self) {
        self.connection.close(0, b"server network handle dropped");
    }
}

#[derive(Debug, Default, Clone)]
struct SharedVoiceQueue(Arc<Mutex<BTreeMap<VoiceStreamId, VecDeque<Vec<u8>>>>>);

#[derive(Debug, Clone)]
struct SharedControlSender {
    sender: tokio_mpsc::Sender<Vec<u8>>,
    queued_bytes: Arc<Mutex<usize>>,
}

impl SharedControlSender {
    fn try_send(&self, frame: Vec<u8>) -> Result<(), QuicError> {
        let frame_bytes = frame.len();
        {
            let mut queued = self
                .queued_bytes
                .lock()
                .map_err(|_poisoned| QuicError::QueueUnavailable)?;
            let next = queued.saturating_add(frame_bytes);
            if next > MAX_CONTROL_QUEUE_BYTES {
                return Err(QuicError::QueueFull);
            }
            *queued = next;
        }
        if self.sender.try_send(frame).is_err() {
            release_control_bytes(&self.queued_bytes, frame_bytes)?;
            return Err(QuicError::QueueFull);
        }
        Ok(())
    }
}

fn control_channel() -> (SharedControlSender, tokio_mpsc::Receiver<Vec<u8>>) {
    let (sender, receiver) = tokio_mpsc::channel(CONTROL_CAPACITY);
    (
        SharedControlSender {
            sender,
            queued_bytes: Arc::new(Mutex::new(0)),
        },
        receiver,
    )
}

fn release_control_bytes(queued: &Mutex<usize>, bytes: usize) -> Result<(), QuicError> {
    let mut queued = queued
        .lock()
        .map_err(|_poisoned| QuicError::QueueUnavailable)?;
    *queued = queued.saturating_sub(bytes);
    Ok(())
}

impl SharedVoiceQueue {
    fn push(&self, stream: VoiceStreamId, datagram: Vec<u8>) -> Result<(), QuicError> {
        let mut streams = self
            .0
            .lock()
            .map_err(|_poisoned| QuicError::QueueUnavailable)?;
        if !streams.contains_key(&stream) && streams.len() >= VOICE_STREAM_CAPACITY {
            return Err(QuicError::QueueFull);
        }
        let queue = streams.entry(stream).or_default();
        if queue.len() == VOICE_PACKETS_PER_STREAM {
            let _expired = queue.pop_front();
        }
        queue.push_back(datagram);
        Ok(())
    }

    fn pop(&self) -> Result<Option<Vec<u8>>, QuicError> {
        let mut streams = self
            .0
            .lock()
            .map_err(|_poisoned| QuicError::QueueUnavailable)?;
        let stream = streams
            .iter()
            .find_map(|(stream, queue)| (!queue.is_empty()).then_some(*stream));
        let Some(stream) = stream else {
            return Ok(None);
        };
        let packet = streams.get_mut(&stream).and_then(VecDeque::pop_front);
        if streams.get(&stream).is_some_and(VecDeque::is_empty) {
            let _empty = streams.remove(&stream);
        }
        Ok(packet)
    }
}

fn spawn_control_tasks(
    connection: quinn::Connection,
    stream: SessionControlStream,
    mut outbound: tokio_mpsc::Receiver<Vec<u8>>,
    queued_bytes: Arc<Mutex<usize>>,
    events: mpsc::SyncSender<NetworkEvent>,
) {
    let SessionControlStream {
        mut send,
        mut receive,
    } = stream;
    let send_connection = connection.clone();
    let send_events = events.clone();
    tokio::spawn(async move {
        while let Some(frame) = outbound.recv().await {
            if release_control_bytes(&queued_bytes, frame.len()).is_err() {
                stop_transport(&send_connection, &send_events);
                return;
            }
            if send.write_all(&frame).await.is_err() {
                stop_transport(&send_connection, &send_events);
                return;
            }
        }
        let _finished = send.finish();
    });
    tokio::spawn(async move {
        loop {
            match read_control_frame(&mut receive).await {
                Ok(frame) => {
                    if !publish(&events, NetworkEvent::SessionControl(frame)) {
                        stop_transport(&connection, &events);
                        return;
                    }
                }
                Err(_) => {
                    stop_transport(&connection, &events);
                    return;
                }
            }
        }
    });
}

fn spawn_datagram_receive(connection: quinn::Connection, events: mpsc::SyncSender<NetworkEvent>) {
    tokio::spawn(async move {
        loop {
            match connection.read_datagram().await {
                Ok(bytes) => {
                    if decode_datagram(&bytes).is_err() {
                        stop_transport(&connection, &events);
                        return;
                    }
                    if !publish(&events, NetworkEvent::Datagram(bytes)) {
                        connection.close(quinn::VarInt::from_u32(2), b"host event queue full");
                        return;
                    }
                }
                Err(_) => {
                    stop_transport(&connection, &events);
                    return;
                }
            }
        }
    });
}

fn spawn_path_change_monitor(
    connection: quinn::Connection,
    events: mpsc::SyncSender<NetworkEvent>,
) {
    tokio::spawn(async move {
        let mut remote_address = connection.remote_address();
        let mut interval = tokio::time::interval(PATH_POLL_INTERVAL);
        loop {
            interval.tick().await;
            let current_address = connection.remote_address();
            if current_address != remote_address {
                if current_address.ip() != remote_address.ip() {
                    stop_transport(&connection, &events);
                    return;
                }
                if !publish(
                    &events,
                    NetworkEvent::PathChanged {
                        previous: remote_address,
                        current: current_address,
                    },
                ) {
                    connection.close(quinn::VarInt::from_u32(2), b"host event queue full");
                    return;
                }
                remote_address = current_address;
            }
            if connection.close_reason().is_some() {
                return;
            }
        }
    });
}

fn spawn_bootstrap_receive(connection: ClientConnection, events: mpsc::SyncSender<NetworkEvent>) {
    tokio::spawn(async move {
        loop {
            match connection.receive_bootstrap().await {
                Ok(transfer) => {
                    if !publish(&events, NetworkEvent::Bootstrap(transfer)) {
                        stop_transport(&connection.inner, &events);
                        return;
                    }
                }
                Err(_) => {
                    stop_transport(&connection.inner, &events);
                    return;
                }
            }
        }
    });
}

fn spawn_client_datagram_send(
    connection: ClientConnection,
    latest_input: Arc<Mutex<Option<Vec<u8>>>>,
    mut time_sync: tokio_mpsc::Receiver<Vec<u8>>,
    voice: SharedVoiceQueue,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(DATAGRAM_POLL_INTERVAL);
        loop {
            interval.tick().await;
            let input = latest_input
                .lock()
                .ok()
                .and_then(|mut latest| latest.take());
            if let Some(datagram) = input {
                let _sent = connection.send_datagram(datagram);
            }
            if let Ok(datagram) = time_sync.try_recv() {
                let _sent = connection.send_datagram(datagram);
            }
            if let Ok(Some(datagram)) = voice.pop() {
                let _sent = connection.send_datagram(datagram);
            }
            if connection.inner.close_reason().is_some() {
                return;
            }
        }
    });
}

fn spawn_server_datagram_send(
    connection: ServerConnection,
    mut snapshots: tokio_mpsc::Receiver<Vec<Vec<u8>>>,
    mut time_sync: tokio_mpsc::Receiver<Vec<u8>>,
    voice: SharedVoiceQueue,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(DATAGRAM_POLL_INTERVAL);
        loop {
            tokio::select! {
                biased;
                _ = interval.tick() => {
                    if let Ok(datagram) = time_sync.try_recv() {
                        let _sent = connection.send_datagram(datagram);
                    }
                    if let Ok(Some(datagram)) = voice.pop() {
                        let _sent = connection.send_datagram(datagram);
                    }
                }
                generation = snapshots.recv() => {
                    let Some(generation) = generation else { return; };
                    for snapshot in generation {
                        if connection.send_datagram(snapshot).is_err() {
                            connection.close(5, b"snapshot send failed");
                            return;
                        }
                    }
                }
            }
        }
    });
}

fn spawn_bootstrap_send(
    connection: ServerConnection,
    mut bootstraps: tokio_mpsc::Receiver<BootstrapTransfer>,
    pending: Arc<AtomicBool>,
) {
    tokio::spawn(async move {
        while let Some(transfer) = bootstraps.recv().await {
            let result = connection
                .send_bootstrap(transfer.header, &transfer.body)
                .await;
            pending.store(false, Ordering::Release);
            if result.is_err() {
                connection.close(3, b"bootstrap failed");
                return;
            }
        }
    });
}

fn validate_control(frame: &[u8]) -> Result<(), QuicError> {
    let _decoded = decode_frame(frame, MAX_CONTROL_MESSAGE_BYTES)?;
    Ok(())
}

fn validate_flow(datagram: &[u8], expected: FlowId) -> Result<(), QuicError> {
    if decode_datagram(datagram)?.header.flow == expected {
        Ok(())
    } else {
        Err(QuicError::StreamRole)
    }
}

fn validate_bootstrap(header: &StateBootstrapHeader, body: &[u8]) -> Result<(), QuicError> {
    let declared = usize::try_from(header.body_length)
        .map_err(|_error| blackflower_networking::WireError::IntegerOutOfRange)?;
    if declared > MAX_BOOTSTRAP_BYTES {
        Err(QuicError::Wire(
            blackflower_networking::WireError::Oversized {
                actual: declared,
                maximum: MAX_BOOTSTRAP_BYTES,
            },
        ))
    } else if body.len() > MAX_BOOTSTRAP_BYTES {
        Err(QuicError::Wire(
            blackflower_networking::WireError::Oversized {
                actual: body.len(),
                maximum: MAX_BOOTSTRAP_BYTES,
            },
        ))
    } else if declared == body.len() {
        Ok(())
    } else {
        Err(QuicError::Wire(if body.len() < declared {
            blackflower_networking::WireError::Truncated
        } else {
            blackflower_networking::WireError::Trailing
        }))
    }
}

fn try_send<T>(sender: &tokio_mpsc::Sender<T>, value: T) -> Result<(), QuicError> {
    sender
        .try_send(value)
        .map_err(|_error| QuicError::QueueFull)
}

fn try_receive(receiver: &mpsc::Receiver<NetworkEvent>) -> Result<Option<NetworkEvent>, QuicError> {
    match receiver.try_recv() {
        Ok(event) => Ok(Some(event)),
        Err(mpsc::TryRecvError::Empty) => Ok(None),
        Err(mpsc::TryRecvError::Disconnected) => Err(QuicError::QueueUnavailable),
    }
}

fn publish(events: &mpsc::SyncSender<NetworkEvent>, event: NetworkEvent) -> bool {
    events.try_send(event).is_ok()
}

fn stop_transport(connection: &quinn::Connection, events: &mpsc::SyncSender<NetworkEvent>) {
    let _published = publish(events, NetworkEvent::TransportStopped);
    connection.close(quinn::VarInt::from_u32(4), b"transport task stopped");
}

const _: () = assert!(MAX_SNAPSHOT_CHUNKS == 4);
