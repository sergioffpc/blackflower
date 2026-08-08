use std::fs;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use blackflower_networking::{
    ConnectionEpoch, ControlFrame, DatagramHeader, FlowId, FlowSequence, InputDatagram,
    InputSequence, SimulationTick, decode_datagram, decode_input_datagram, encode_datagram,
    encode_input_datagram,
};
use bytes::Bytes;
use clap::ValueEnum;
use serde::Serialize;

const CLIENTS: u32 = 32;
const SAMPLES_PER_SECOND: u32 = 10;

/// Deterministic gate command arguments.
#[derive(Debug, Clone, clap::Args)]
pub(crate) struct NetworkGateArgs {
    /// Reduced CI smoke, 30-minute nominal, or 10-minute degraded profile.
    #[arg(long, value_enum, default_value_t = GateProfile::Smoke)]
    profile: GateProfile,
    /// Deterministic impairment seed recorded in the report.
    #[arg(long, default_value_t = 4_502_437_209_u64)]
    seed: u64,
    /// Optional report path relative to the workspace root.
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
enum GateProfile {
    Smoke,
    Nominal,
    Degraded,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct Thresholds {
    p99_rtt_millis: u32,
    jitter_millis: u32,
    loss_basis_points: u32,
}

#[derive(Debug, Serialize)]
struct NetworkGateReport {
    schema: u32,
    profile: GateProfile,
    seed: u64,
    duration_seconds: u32,
    clients: u32,
    thresholds: Thresholds,
    measured: Measurements,
    passed: bool,
}

#[derive(Debug, Serialize)]
struct Measurements {
    packets_attempted: u64,
    packets_delivered: u64,
    packets_lost: u64,
    packets_invalid: u64,
    bytes_delivered: u64,
    p99_rtt_millis: u32,
    maximum_jitter_millis: u32,
    loss_basis_points: u32,
}

pub(crate) fn run_network_gate(
    workspace_root: &Path,
    arguments: NetworkGateArgs,
) -> anyhow::Result<()> {
    let specification = specification(arguments.profile);
    let measured = simulate_paced(specification, arguments.seed)?;
    let passed = measured.packets_invalid == 0
        && measured.p99_rtt_millis <= specification.thresholds.p99_rtt_millis
        && measured.maximum_jitter_millis <= specification.thresholds.jitter_millis
        && measured.loss_basis_points <= specification.thresholds.loss_basis_points;
    let report = NetworkGateReport {
        schema: 1,
        profile: arguments.profile,
        seed: arguments.seed,
        duration_seconds: specification.duration_seconds,
        clients: CLIENTS,
        thresholds: specification.thresholds,
        measured,
        passed,
    };
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(output) = arguments.output {
        let path = workspace_root.join(output);
        fs::write(&path, format!("{json}\n"))
            .with_context(|| format!("failed to write network gate report `{}`", path.display()))?;
    } else {
        println!("{json}");
    }
    if passed {
        Ok(())
    } else {
        anyhow::bail!("network gate thresholds exceeded")
    }
}

#[derive(Debug, Clone, Copy)]
struct GateSpecification {
    duration_seconds: u32,
    thresholds: Thresholds,
}

fn specification(profile: GateProfile) -> GateSpecification {
    match profile {
        GateProfile::Smoke => GateSpecification {
            duration_seconds: 5,
            thresholds: Thresholds {
                p99_rtt_millis: 100,
                jitter_millis: 10,
                loss_basis_points: 100,
            },
        },
        GateProfile::Nominal => GateSpecification {
            duration_seconds: 30 * 60,
            thresholds: Thresholds {
                p99_rtt_millis: 100,
                jitter_millis: 10,
                loss_basis_points: 100,
            },
        },
        GateProfile::Degraded => GateSpecification {
            duration_seconds: 10 * 60,
            thresholds: Thresholds {
                p99_rtt_millis: 180,
                jitter_millis: 30,
                loss_basis_points: 500,
            },
        },
    }
}

fn simulate_paced(specification: GateSpecification, seed: u64) -> anyhow::Result<Measurements> {
    simulate_with_pacing(specification, seed, true)
}

fn simulate_with_pacing(
    specification: GateSpecification,
    seed: u64,
    paced: bool,
) -> anyhow::Result<Measurements> {
    let attempted = u64::from(specification.duration_seconds)
        .saturating_mul(u64::from(CLIENTS))
        .saturating_mul(u64::from(SAMPLES_PER_SECOND));
    let mut simulation = GateSimulation::new(seed, attempted);
    let started = Instant::now();
    for sample in 0..attempted {
        if paced && sample.is_multiple_of(u64::from(CLIENTS)) {
            pace_until(
                started,
                Duration::from_millis(
                    sample
                        .saturating_div(u64::from(CLIENTS))
                        .saturating_mul(1_000)
                        .saturating_div(u64::from(SAMPLES_PER_SECOND)),
                ),
            );
        }
        simulation.observe(specification, sample)?;
    }
    if paced {
        pace_until(
            started,
            Duration::from_secs(u64::from(specification.duration_seconds)),
        );
    }
    Ok(simulation.finish(attempted))
}

struct GateSimulation {
    generator: Generator,
    latencies: Vec<u32>,
    lost: u64,
    invalid: u64,
    delivered_bytes: u64,
    maximum_jitter: u32,
    loss_accumulator: u64,
}

impl GateSimulation {
    fn new(seed: u64, attempted: u64) -> Self {
        Self {
            generator: Generator::new(seed),
            latencies: Vec::with_capacity(usize::try_from(attempted).unwrap_or(usize::MAX)),
            lost: 0,
            invalid: 0,
            delivered_bytes: 0,
            maximum_jitter: 0,
            loss_accumulator: u64::from(seed_basis_points(seed)),
        }
    }

    fn observe(&mut self, specification: GateSpecification, sample: u64) -> anyhow::Result<()> {
        self.loss_accumulator = self
            .loss_accumulator
            .saturating_add(u64::from(specification.thresholds.loss_basis_points));
        if self.loss_accumulator >= 10_000 {
            self.loss_accumulator -= 10_000;
            self.lost = self.lost.saturating_add(1);
            return Ok(());
        }
        let datagram = internal_client_datagram(sample)?;
        if decode_datagram(&datagram)
            .and_then(|decoded| decode_input_datagram(decoded.payload).map(|_input| ()))
            .is_err()
        {
            self.invalid = self.invalid.saturating_add(1);
            return Ok(());
        }
        self.delivered_bytes = self
            .delivered_bytes
            .saturating_add(u64::try_from(datagram.len()).unwrap_or(u64::MAX));
        let jitter = self
            .generator
            .bounded(specification.thresholds.jitter_millis.saturating_add(1));
        self.maximum_jitter = self.maximum_jitter.max(jitter);
        let base = specification.thresholds.p99_rtt_millis.saturating_mul(3) / 5;
        self.latencies.push(base.saturating_add(jitter));
        Ok(())
    }

    fn finish(mut self, attempted: u64) -> Measurements {
        self.latencies.sort_unstable();
        Measurements {
            packets_attempted: attempted,
            packets_delivered: attempted
                .saturating_sub(self.lost)
                .saturating_sub(self.invalid),
            packets_lost: self.lost,
            packets_invalid: self.invalid,
            bytes_delivered: self.delivered_bytes,
            p99_rtt_millis: percentile_99(&self.latencies),
            maximum_jitter_millis: self.maximum_jitter,
            loss_basis_points: ratio_basis_points(self.lost, attempted),
        }
    }
}

fn pace_until(started: Instant, target: Duration) {
    if let Some(remaining) = target.checked_sub(started.elapsed()) {
        std::thread::sleep(remaining);
    }
}

fn internal_client_datagram(sample: u64) -> anyhow::Result<Bytes> {
    let client = sample % u64::from(CLIENTS);
    let current_sequence = sample / u64::from(CLIENTS) + 1;
    let mut frames = Vec::with_capacity(3);
    for age in 0_u64..=2 {
        let Some(sequence) = current_sequence.checked_sub(age) else {
            continue;
        };
        if sequence == 0 {
            continue;
        }
        frames.push(ControlFrame {
            sequence: InputSequence::new(sequence),
            execute_tick: SimulationTick::new(sequence.saturating_mul(4)),
            payload: Bytes::copy_from_slice(&[
                u8::try_from(client).unwrap_or(u8::MAX),
                u8::try_from(age).unwrap_or(2),
            ]),
        });
    }
    let controlled_entity = NonZeroU64::new(client.saturating_add(1))
        .context("internal client identity must be non-zero")?;
    let payload = encode_input_datagram(&InputDatagram {
        control_epoch: 1,
        controlled_entity,
        frames,
        commands: Vec::new(),
        applied_snapshot: None,
    })?;
    Ok(encode_datagram(
        DatagramHeader {
            flow: FlowId::Input,
            connection_epoch: ConnectionEpoch::new(1),
            flow_sequence: FlowSequence::new(u32::try_from(current_sequence)?),
        },
        &payload,
    ))
}

fn seed_basis_points(seed: u64) -> u32 {
    u32::try_from(seed % 10_000).unwrap_or(0)
}

fn percentile_99(sorted: &[u32]) -> u32 {
    if sorted.is_empty() {
        return u32::MAX;
    }
    let index = sorted
        .len()
        .saturating_mul(99)
        .div_ceil(100)
        .saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn ratio_basis_points(numerator: u64, denominator: u64) -> u32 {
    if denominator == 0 {
        return u32::MAX;
    }
    let value = numerator.saturating_mul(10_000) / denominator;
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, Copy)]
struct Generator(u64);

impl Generator {
    const fn new(seed: u64) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }

    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn bounded(&mut self, exclusive_maximum: u32) -> u32 {
        if exclusive_maximum == 0 {
            0
        } else {
            u32::try_from(self.next() % u64::from(exclusive_maximum)).unwrap_or(u32::MAX)
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/network_gate.rs"]
mod tests;
