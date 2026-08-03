use std::io;
use std::thread;
use std::time::{Duration, Instant};

use blackflower_world_simulation::SIMULATION_TICK_RATE_HZ;

use super::{NANOSECONDS_PER_SECOND, SimulationHost, TickPacer};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn pacer_accumulates_exactly_one_second_for_one_tick_rate_window() {
    let started = Instant::now();
    let mut pacer = TickPacer::new(started);
    for _tick in 0..SIMULATION_TICK_RATE_HZ {
        pacer.advance();
    }
    assert_eq!(
        pacer.deadline.duration_since(started).as_nanos(),
        u128::from(NANOSECONDS_PER_SECOND),
    );
}

#[test]
fn simulation_host_ticks_until_orderly_shutdown() -> TestResult {
    let host = SimulationHost::spawn()?;
    wait_for_ticks(&host, 2)?;
    let exit = host.shutdown()?;
    assert!(exit.completed_ticks >= 2);
    Ok(())
}

fn wait_for_ticks(host: &SimulationHost, expected: u64) -> TestResult {
    let deadline = Instant::now() + Duration::from_millis(500);
    while host.completed_ticks() < expected {
        if Instant::now() >= deadline {
            return Err(io::Error::other("simulation host did not advance").into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
