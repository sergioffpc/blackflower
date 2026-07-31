use blackflower_ecs::{RunError, TickDelta};

use crate::{SimulationPhase, SimulationTick};

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
