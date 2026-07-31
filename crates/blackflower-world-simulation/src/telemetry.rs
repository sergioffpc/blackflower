use blackflower_ecs::{RunError, TickDelta};

use crate::{SimulationPhase, SimulationTick};
use blackflower_acoustics::AcousticFrame;

pub(crate) struct TickObservation {
    #[cfg(feature = "metrics")]
    started: std::time::Instant,
    #[cfg(feature = "metrics")]
    deadline: std::time::Duration,
}

impl TickObservation {
    pub(crate) fn start(delta: TickDelta) -> Self {
        #[cfg(not(feature = "metrics"))]
        let _ = delta;

        Self {
            #[cfg(feature = "metrics")]
            started: std::time::Instant::now(),
            #[cfg(feature = "metrics")]
            deadline: std::time::Duration::from_secs_f32(delta.as_seconds()),
        }
    }

    pub(crate) fn finish(self, _result: &Result<bool, RunError>) {
        #[cfg(any(feature = "metrics", feature = "tracing"))]
        let outcome = tick_outcome(_result);

        #[cfg(feature = "metrics")]
        {
            let elapsed = self.started.elapsed();
            metrics::counter!(
                "blackflower_world_simulation_ticks_total",
                "result" => outcome,
            )
            .increment(1);
            metrics::histogram!("blackflower_world_simulation_tick_duration_seconds")
                .record(elapsed.as_secs_f64());
            if elapsed > self.deadline {
                metrics::counter!("blackflower_world_simulation_deadline_misses_total")
                    .increment(1);
            }
        }

        #[cfg(feature = "tracing")]
        tracing::Span::current().record("result", outcome);
    }
}

pub(crate) fn system_executed(phase: SimulationPhase, system: &'static str, tick: SimulationTick) {
    #[cfg(feature = "metrics")]
    metrics::counter!(
        "blackflower_world_simulation_system_executions_total",
        "phase" => phase.name(),
    )
    .increment(1);

    #[cfg(feature = "tracing")]
    tracing::trace!(
        target: "blackflower_world_simulation",
        phase = phase.name(),
        system,
        tick = tick.get(),
        "system executed",
    );

    #[cfg(not(any(feature = "metrics", feature = "tracing")))]
    let _ = (phase, system, tick);
}

pub(crate) fn acoustic_frame(frame: &AcousticFrame) {
    #[cfg(feature = "metrics")]
    {
        let candidate_pairs = u32::try_from(frame.candidate_pairs).unwrap_or(u32::MAX);
        let direct_pairs = u32::try_from(frame.direct_pairs).unwrap_or(u32::MAX);
        metrics::histogram!("blackflower_acoustic_candidate_pairs")
            .record(f64::from(candidate_pairs));
        metrics::histogram!("blackflower_acoustic_direct_pairs").record(f64::from(direct_pairs));
        metrics::counter!("blackflower_acoustic_observations_total")
            .increment(u64::try_from(frame.observations.len()).unwrap_or(u64::MAX));
        metrics::counter!("blackflower_acoustic_sound_deliveries_total")
            .increment(u64::try_from(frame.sounds.len()).unwrap_or(u64::MAX));
        metrics::counter!("blackflower_acoustic_voice_deliveries_total")
            .increment(u64::try_from(frame.voices.len()).unwrap_or(u64::MAX));
        metrics::counter!("blackflower_acoustic_deferred_indirect_pairs_total")
            .increment(u64::try_from(frame.deferred_indirect_pairs).unwrap_or(u64::MAX));
    }

    #[cfg(feature = "tracing")]
    tracing::debug!(
        target: "blackflower_world_simulation",
        structure_version = frame.structure_version.0,
        direct_pairs = frame.direct_pairs,
        candidate_pairs = frame.candidate_pairs,
        observations = frame.observations.len(),
        sound_deliveries = frame.sounds.len(),
        voice_deliveries = frame.voices.len(),
        deferred_indirect_pairs = frame.deferred_indirect_pairs,
        "authoritative acoustic frame sealed",
    );

    #[cfg(not(any(feature = "metrics", feature = "tracing")))]
    let _ = frame;
}

#[allow(
    clippy::too_many_lines,
    reason = "metric declarations stay adjacent so names, units, and descriptions remain auditable"
)]
pub(crate) fn describe_metrics() {
    #[cfg(feature = "metrics")]
    {
        use metrics::Unit;

        metrics::describe_counter!(
            "blackflower_world_simulation_ticks_total",
            Unit::Count,
            "Authoritative simulation tick executions",
        );
        metrics::describe_counter!(
            "blackflower_world_simulation_deadline_misses_total",
            Unit::Count,
            "Authoritative simulation ticks that exceeded their fixed-step budget",
        );
        metrics::describe_counter!(
            "blackflower_world_simulation_system_executions_total",
            Unit::Count,
            "Authoritative simulation system executions by phase",
        );
        metrics::describe_histogram!(
            "blackflower_world_simulation_tick_duration_seconds",
            Unit::Seconds,
            "Wall-clock duration of an authoritative simulation tick",
        );
        metrics::describe_histogram!(
            "blackflower_acoustic_candidate_pairs",
            Unit::Count,
            "Source and receiver pairs resolved by one authoritative acoustic tick",
        );
        metrics::describe_histogram!(
            "blackflower_acoustic_direct_pairs",
            Unit::Count,
            "Direct and transmission pairs preserved by one authoritative acoustic tick",
        );
        metrics::describe_counter!(
            "blackflower_acoustic_observations_total",
            Unit::Count,
            "Privacy-preserving bot acoustic observations",
        );
        metrics::describe_counter!(
            "blackflower_acoustic_sound_deliveries_total",
            Unit::Count,
            "Server-gated physical sound deliveries",
        );
        metrics::describe_counter!(
            "blackflower_acoustic_voice_deliveries_total",
            Unit::Count,
            "Server-gated physical voice deliveries",
        );
        metrics::describe_counter!(
            "blackflower_acoustic_deferred_indirect_pairs_total",
            Unit::Count,
            "Indirect acoustic refinements deferred by deterministic budgets",
        );
    }
}

#[cfg(any(feature = "metrics", feature = "tracing"))]
fn tick_outcome(result: &Result<bool, RunError>) -> &'static str {
    match result {
        Ok(true) => "completed",
        Ok(false) => "stopped",
        Err(_) => "failed",
    }
}
