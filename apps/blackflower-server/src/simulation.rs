use std::io;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use blackflower_world_simulation::{
    ActorId, MovementControl, MovementFrame, SIMULATION_TICK_RATE_HZ, SimulationWorld,
};

mod telemetry;

const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;
const SIMULATION_INGRESS_CAPACITY: usize = 4_096;
const MAX_INGRESS_COMMANDS_PER_TICK: usize = 1_024;

/// Result of an orderly dedicated simulation-host shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationExit {
    /// Number of authoritative ticks completed by this process.
    pub completed_ticks: u64,
}

/// Failure while starting, running, or joining the authoritative simulation host.
#[derive(Debug, thiserror::Error)]
pub enum SimulationHostError {
    /// The operating system rejected creation of the simulation thread.
    #[error("failed to spawn authoritative simulation thread")]
    ThreadSpawn(#[source] io::Error),
    /// `SimulationWorld` could not be initialized on its owning thread.
    #[error("authoritative simulation initialization failed: {0}")]
    Initialization(String),
    /// One authoritative simulation tick failed.
    #[error("authoritative simulation tick failed: {0}")]
    Tick(String),
    /// The simulation thread terminated without reporting initialization.
    #[error("authoritative simulation initialization channel closed")]
    InitializationChannelClosed,
    /// The simulation thread panicked.
    #[error("authoritative simulation thread panicked")]
    ThreadPanicked,
    /// Movement ingress or state exchange failed on the owning thread.
    #[error("authoritative movement runtime failed: {0}")]
    Movement(String),
}

/// Process-owned fixed-rate host for the non-`Send` authoritative world.
pub struct SimulationHost {
    stop: Arc<AtomicBool>,
    completed_ticks: Arc<AtomicU64>,
    ingress: SyncSender<SimulationCommand>,
    movement_frame: Arc<Mutex<MovementFrame>>,
    worker: Option<JoinHandle<Result<SimulationExit, SimulationHostError>>>,
}

/// Cloneable bounded ingress and sealed-state handle for network and diagnostics tasks.
#[derive(Debug, Clone)]
pub struct SimulationStatus {
    completed_ticks: Arc<AtomicU64>,
    ingress: SyncSender<SimulationCommand>,
    movement_frame: Arc<Mutex<MovementFrame>>,
}

impl SimulationStatus {
    /// Return the latest completed authoritative tick count.
    #[must_use]
    pub fn completed_ticks(&self) -> u64 {
        self.completed_ticks.load(Ordering::Acquire)
    }

    /// Request creation of one controllable actor on the simulation thread.
    pub fn try_spawn_actor(&self, actor: ActorId) -> Result<(), SimulationIngressError> {
        self.try_send(SimulationCommand::Spawn(actor))
    }

    /// Request removal of one controllable actor and its pending inputs.
    pub fn try_despawn_actor(&self, actor: ActorId) -> Result<(), SimulationIngressError> {
        self.try_send(SimulationCommand::Despawn(actor))
    }

    /// Submit canonical controls without blocking the network runtime.
    pub fn try_submit_controls(
        &self,
        controls: Vec<MovementControl>,
    ) -> Result<(), SimulationIngressError> {
        if controls.is_empty() {
            return Ok(());
        }
        self.try_send(SimulationCommand::Controls(controls))
    }

    /// Clone the latest movement frame sealed by the simulation thread.
    pub fn movement_frame(&self) -> Result<MovementFrame, SimulationIngressError> {
        self.movement_frame
            .lock()
            .map(|frame| frame.clone())
            .map_err(|_error| SimulationIngressError::StatePoisoned)
    }

    fn try_send(&self, command: SimulationCommand) -> Result<(), SimulationIngressError> {
        self.ingress.try_send(command).map_err(|error| match error {
            TrySendError::Full(_command) => SimulationIngressError::Full,
            TrySendError::Disconnected(_command) => SimulationIngressError::Stopped,
        })
    }
}

impl SimulationHost {
    /// Create `SimulationWorld` on its owning thread and begin 240 Hz execution.
    pub fn spawn() -> Result<Self, SimulationHostError> {
        let stop = Arc::new(AtomicBool::new(false));
        let completed_ticks = Arc::new(AtomicU64::new(0));
        let (ingress, ingress_receiver) = mpsc::sync_channel(SIMULATION_INGRESS_CAPACITY);
        let movement_frame = Arc::new(Mutex::new(MovementFrame::default()));
        let (initialized_send, initialized_receive) = mpsc::sync_channel(1);
        let worker = spawn_worker(
            Arc::clone(&stop),
            Arc::clone(&completed_ticks),
            ingress_receiver,
            Arc::clone(&movement_frame),
            initialized_send,
        )?;
        match initialized_receive.recv() {
            Ok(Ok(())) => Ok(Self {
                stop,
                completed_ticks,
                ingress,
                movement_frame,
                worker: Some(worker),
            }),
            Ok(Err(message)) => {
                drop(worker.join());
                Err(SimulationHostError::Initialization(message))
            }
            Err(_error) => {
                drop(worker.join());
                Err(SimulationHostError::InitializationChannelClosed)
            }
        }
    }

    /// Return the latest completed authoritative tick count.
    #[must_use]
    pub fn completed_ticks(&self) -> u64 {
        self.completed_ticks.load(Ordering::Acquire)
    }

    /// Return a cloneable bounded ingress and sealed-state handle.
    #[must_use]
    pub fn status(&self) -> SimulationStatus {
        SimulationStatus {
            completed_ticks: Arc::clone(&self.completed_ticks),
            ingress: self.ingress.clone(),
            movement_frame: Arc::clone(&self.movement_frame),
        }
    }

    /// Request orderly shutdown and join the simulation thread.
    pub fn shutdown(mut self) -> Result<SimulationExit, SimulationHostError> {
        self.stop.store(true, Ordering::Release);
        self.join_worker()
    }

    fn join_worker(&mut self) -> Result<SimulationExit, SimulationHostError> {
        let worker = self
            .worker
            .take()
            .ok_or(SimulationHostError::ThreadPanicked)?;
        worker
            .join()
            .map_err(|_panic| SimulationHostError::ThreadPanicked)?
    }
}

impl Drop for SimulationHost {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
    }
}

fn spawn_worker(
    stop: Arc<AtomicBool>,
    completed_ticks: Arc<AtomicU64>,
    ingress: Receiver<SimulationCommand>,
    movement_frame: Arc<Mutex<MovementFrame>>,
    initialized: mpsc::SyncSender<Result<(), String>>,
) -> Result<JoinHandle<Result<SimulationExit, SimulationHostError>>, SimulationHostError> {
    thread::Builder::new()
        .name("blackflower-simulation".to_owned())
        .spawn(move || {
            run_worker(
                &stop,
                &completed_ticks,
                &ingress,
                &movement_frame,
                &initialized,
            )
        })
        .map_err(SimulationHostError::ThreadSpawn)
}

fn run_worker(
    stop: &AtomicBool,
    completed_ticks: &AtomicU64,
    ingress: &Receiver<SimulationCommand>,
    movement_frame: &Mutex<MovementFrame>,
    initialized: &mpsc::SyncSender<Result<(), String>>,
) -> Result<SimulationExit, SimulationHostError> {
    let mut simulation = match SimulationWorld::new() {
        Ok(simulation) => simulation,
        Err(error) => return initialization_failed(initialized, error.to_string()),
    };
    initialized
        .send(Ok(()))
        .map_err(|_error| SimulationHostError::InitializationChannelClosed)?;
    run_ticks(
        &mut simulation,
        stop,
        completed_ticks,
        ingress,
        movement_frame,
    )
}

fn initialization_failed(
    initialized: &mpsc::SyncSender<Result<(), String>>,
    message: String,
) -> Result<SimulationExit, SimulationHostError> {
    drop(initialized.send(Err(message.clone())));
    Err(SimulationHostError::Initialization(message))
}

fn run_ticks(
    simulation: &mut SimulationWorld,
    stop: &AtomicBool,
    completed_ticks: &AtomicU64,
    ingress: &Receiver<SimulationCommand>,
    movement_frame: &Mutex<MovementFrame>,
) -> Result<SimulationExit, SimulationHostError> {
    let mut pacer = TickPacer::new(Instant::now());
    while !stop.load(Ordering::Acquire) {
        let timing = pacer.wait();
        if stop.load(Ordering::Acquire) {
            break;
        }
        telemetry::tick_started(timing.waited, timing.lag, timing.ticks_behind);
        drain_ingress(simulation, ingress)?;
        let result = simulation.tick();
        telemetry::tick_finished(timing.deadline_pressure_ratio(Instant::now()));
        let should_continue =
            result.map_err(|error| SimulationHostError::Tick(error.to_string()))?;
        let sealed = simulation
            .movement_frame()
            .map_err(|error| SimulationHostError::Movement(error.to_string()))?;
        *movement_frame.lock().map_err(|_error| {
            SimulationHostError::Movement("state lock is poisoned".to_owned())
        })? = sealed;
        completed_ticks.store(simulation.current_tick().get(), Ordering::Release);
        if !should_continue {
            break;
        }
        pacer.advance();
    }
    Ok(SimulationExit {
        completed_ticks: completed_ticks.load(Ordering::Acquire),
    })
}

fn drain_ingress(
    simulation: &mut SimulationWorld,
    ingress: &Receiver<SimulationCommand>,
) -> Result<(), SimulationHostError> {
    for _index in 0..MAX_INGRESS_COMMANDS_PER_TICK {
        let command = match ingress.try_recv() {
            Ok(command) => command,
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        };
        match command {
            SimulationCommand::Spawn(actor) => simulation
                .spawn_movement_actor(actor)
                .map_err(|error| SimulationHostError::Movement(error.to_string()))?,
            SimulationCommand::Despawn(actor) => {
                let _removed = simulation
                    .despawn_movement_actor(actor)
                    .map_err(|error| SimulationHostError::Movement(error.to_string()))?;
            }
            SimulationCommand::Controls(controls) => {
                for control in controls {
                    let _accepted = simulation
                        .submit_movement_control(control)
                        .map_err(|error| SimulationHostError::Movement(error.to_string()))?;
                }
            }
        }
    }
    Ok(())
}

enum SimulationCommand {
    Spawn(ActorId),
    Despawn(ActorId),
    Controls(Vec<MovementControl>),
}

/// Bounded simulation-ingress or latest-state exchange failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SimulationIngressError {
    /// The simulation input queue is full for this service cycle.
    #[error("authoritative simulation ingress queue is full")]
    Full,
    /// The simulation owning thread has stopped.
    #[error("authoritative simulation has stopped")]
    Stopped,
    /// The sealed movement-frame mailbox is poisoned.
    #[error("authoritative simulation state mailbox is poisoned")]
    StatePoisoned,
}

struct TickPacer {
    deadline: Instant,
    remainder: u64,
}

impl TickPacer {
    fn new(now: Instant) -> Self {
        telemetry::initialize();
        Self {
            deadline: now,
            remainder: 0,
        }
    }

    fn wait(&self) -> TickTiming {
        let before = Instant::now();
        let waited = if let Some(requested) = self.deadline.checked_duration_since(before) {
            thread::sleep(requested);
            requested
        } else {
            Duration::ZERO
        };
        self.timing_at(Instant::now(), waited)
    }

    fn timing_at(&self, started_at: Instant, waited: Duration) -> TickTiming {
        let lag = started_at
            .checked_duration_since(self.deadline)
            .unwrap_or_default();
        TickTiming {
            deadline: self.deadline,
            waited,
            lag,
            ticks_behind: full_ticks(lag),
        }
    }

    fn advance(&mut self) {
        let base = NANOSECONDS_PER_SECOND / SIMULATION_TICK_RATE_HZ;
        self.remainder += NANOSECONDS_PER_SECOND % SIMULATION_TICK_RATE_HZ;
        let carry = u64::from(self.remainder >= SIMULATION_TICK_RATE_HZ);
        if carry != 0 {
            self.remainder -= SIMULATION_TICK_RATE_HZ;
        }
        self.deadline += Duration::from_nanos(base + carry);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TickTiming {
    deadline: Instant,
    waited: Duration,
    lag: Duration,
    ticks_behind: u64,
}

impl TickTiming {
    fn deadline_pressure_ratio(self, finished_at: Instant) -> f64 {
        let occupied = finished_at
            .checked_duration_since(self.deadline)
            .unwrap_or_default();
        occupied.as_secs_f64() * tick_rate_f64()
    }
}

fn full_ticks(duration: Duration) -> u64 {
    let ticks = duration
        .as_nanos()
        .saturating_mul(u128::from(SIMULATION_TICK_RATE_HZ))
        / u128::from(NANOSECONDS_PER_SECOND);
    u64::try_from(ticks).unwrap_or(u64::MAX)
}

fn tick_rate_f64() -> f64 {
    let rate = u32::try_from(SIMULATION_TICK_RATE_HZ).unwrap_or(u32::MAX);
    f64::from(rate)
}

#[cfg(test)]
#[path = "../tests/unit/simulation.rs"]
mod tests;
