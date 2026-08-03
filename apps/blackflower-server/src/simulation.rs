use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use blackflower_world_simulation::{SIMULATION_TICK_RATE_HZ, SimulationWorld};

const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;

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
}

/// Process-owned fixed-rate host for the non-`Send` authoritative world.
pub struct SimulationHost {
    stop: Arc<AtomicBool>,
    completed_ticks: Arc<AtomicU64>,
    worker: Option<JoinHandle<Result<SimulationExit, SimulationHostError>>>,
}

impl SimulationHost {
    /// Create `SimulationWorld` on its owning thread and begin 240 Hz execution.
    pub fn spawn() -> Result<Self, SimulationHostError> {
        let stop = Arc::new(AtomicBool::new(false));
        let completed_ticks = Arc::new(AtomicU64::new(0));
        let (initialized_send, initialized_receive) = mpsc::sync_channel(1);
        let worker = spawn_worker(
            Arc::clone(&stop),
            Arc::clone(&completed_ticks),
            initialized_send,
        )?;
        match initialized_receive.recv() {
            Ok(Ok(())) => Ok(Self {
                stop,
                completed_ticks,
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
    initialized: mpsc::SyncSender<Result<(), String>>,
) -> Result<JoinHandle<Result<SimulationExit, SimulationHostError>>, SimulationHostError> {
    thread::Builder::new()
        .name("blackflower-simulation".to_owned())
        .spawn(move || run_worker(&stop, &completed_ticks, &initialized))
        .map_err(SimulationHostError::ThreadSpawn)
}

fn run_worker(
    stop: &AtomicBool,
    completed_ticks: &AtomicU64,
    initialized: &mpsc::SyncSender<Result<(), String>>,
) -> Result<SimulationExit, SimulationHostError> {
    let mut simulation = match SimulationWorld::new() {
        Ok(simulation) => simulation,
        Err(error) => return initialization_failed(initialized, error.to_string()),
    };
    initialized
        .send(Ok(()))
        .map_err(|_error| SimulationHostError::InitializationChannelClosed)?;
    run_ticks(&mut simulation, stop, completed_ticks)
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
) -> Result<SimulationExit, SimulationHostError> {
    let mut pacer = TickPacer::new(Instant::now());
    while !stop.load(Ordering::Acquire) {
        pacer.wait();
        if stop.load(Ordering::Acquire) {
            break;
        }
        let should_continue = simulation
            .tick()
            .map_err(|error| SimulationHostError::Tick(error.to_string()))?;
        completed_ticks.fetch_add(1, Ordering::AcqRel);
        if !should_continue {
            break;
        }
        pacer.advance();
    }
    Ok(SimulationExit {
        completed_ticks: completed_ticks.load(Ordering::Acquire),
    })
}

struct TickPacer {
    deadline: Instant,
    remainder: u64,
}

impl TickPacer {
    fn new(now: Instant) -> Self {
        Self {
            deadline: now,
            remainder: 0,
        }
    }

    fn wait(&self) {
        if let Some(remaining) = self.deadline.checked_duration_since(Instant::now()) {
            thread::sleep(remaining);
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

#[cfg(test)]
#[path = "../tests/unit/simulation.rs"]
mod tests;
