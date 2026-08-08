#[cfg(any(feature = "metrics", feature = "tracing"))]
use crate::HardResyncReason;
use crate::{
    PredictionError, PredictionExecution, PredictionPass, PredictionPhase, ReconciliationError,
    ReconciliationOutcome,
};

pub(crate) struct TickObservation {
    #[cfg(feature = "metrics")]
    pass: &'static str,
    #[cfg(feature = "metrics")]
    started: std::time::Instant,
}

impl TickObservation {
    pub(crate) fn start(pass: PredictionPass) -> Self {
        #[cfg(not(feature = "metrics"))]
        let _ = pass;

        Self {
            #[cfg(feature = "metrics")]
            pass: pass_name(pass),
            #[cfg(feature = "metrics")]
            started: std::time::Instant::now(),
        }
    }

    pub(crate) fn finish(self, _result: &Result<bool, PredictionError>) {
        #[cfg(any(feature = "metrics", feature = "tracing"))]
        let outcome = tick_outcome(_result);

        #[cfg(feature = "metrics")]
        {
            metrics::counter!(
                "blackflower_world_prediction_ticks_total",
                "pass" => self.pass,
                "result" => outcome,
            )
            .increment(1);
            metrics::histogram!(
                "blackflower_world_prediction_tick_duration_seconds",
                "pass" => self.pass,
            )
            .record(self.started.elapsed().as_secs_f64());
        }

        #[cfg(feature = "tracing")]
        tracing::Span::current().record("result", outcome);
    }
}

pub(crate) struct ReconciliationObservation {
    #[cfg(feature = "metrics")]
    started: std::time::Instant,
}

impl ReconciliationObservation {
    pub(crate) fn start() -> Self {
        Self {
            #[cfg(feature = "metrics")]
            started: std::time::Instant::now(),
        }
    }

    pub(crate) fn finish<E>(self, _result: &Result<ReconciliationOutcome, ReconciliationError<E>>)
    where
        E: std::error::Error + 'static,
    {
        #[cfg(any(feature = "metrics", feature = "tracing"))]
        let (outcome, reason) = reconciliation_result(_result);

        #[cfg(feature = "metrics")]
        {
            metrics::counter!(
                "blackflower_world_prediction_reconciliations_total",
                "result" => outcome,
                "reason" => reason,
            )
            .increment(1);
            metrics::histogram!("blackflower_world_prediction_reconciliation_duration_seconds")
                .record(self.started.elapsed().as_secs_f64());
            if let Ok(ReconciliationOutcome::Reconciled {
                resimulated_ticks, ..
            }) = _result
            {
                metrics::histogram!("blackflower_world_prediction_resimulated_ticks")
                    .record(histogram_count(*resimulated_ticks));
            }
        }

        #[cfg(feature = "tracing")]
        {
            let span = tracing::Span::current();
            span.record("result", outcome);
            span.record("reason", reason);
        }

        #[cfg(feature = "tracing")]
        if let Ok(ReconciliationOutcome::HardResyncRequired { .. }) = _result {
            tracing::warn!(
                target: "blackflower_world_prediction",
                result = outcome,
                reason,
                "hard resync required",
            );
        }

        #[cfg(feature = "tracing")]
        if let Err(error) = _result {
            tracing::error!(
                target: "blackflower_world_prediction",
                result = outcome,
                reason,
                error = %error,
                "reconciliation failed",
            );
        }
    }
}

pub(crate) fn tick_rejected(pass: PredictionPass, reason: &'static str) {
    #[cfg(feature = "metrics")]
    metrics::counter!(
        "blackflower_world_prediction_ticks_total",
        "pass" => pass_name(pass),
        "result" => "rejected",
    )
    .increment(1);

    #[cfg(feature = "tracing")]
    {
        let span = tracing::Span::current();
        span.record("result", "rejected");
        span.record("reason", reason);
        tracing::error!(
            target: "blackflower_world_prediction",
            pass = pass_name(pass),
            reason,
            "tick rejected",
        );
    }

    #[cfg(not(any(feature = "metrics", feature = "tracing")))]
    let _ = (pass, reason);
}

pub(crate) fn system_executed(
    phase: PredictionPhase,
    system: &'static str,
    execution: PredictionExecution,
) {
    #[cfg(feature = "metrics")]
    metrics::counter!(
        "blackflower_world_prediction_system_executions_total",
        "phase" => phase.name(),
        "pass" => pass_name(execution.pass),
    )
    .increment(1);

    #[cfg(feature = "tracing")]
    tracing::trace!(
        target: "blackflower_world_prediction",
        phase = phase.name(),
        system,
        tick = execution.tick.get(),
        pass = pass_name(execution.pass),
        "system executed",
    );

    #[cfg(not(any(feature = "metrics", feature = "tracing")))]
    let _ = (phase, system, execution);
}

pub(crate) fn describe_metrics() {
    #[cfg(feature = "metrics")]
    {
        use metrics::Unit;

        metrics::describe_counter!(
            "blackflower_world_prediction_ticks_total",
            Unit::Count,
            "Prediction pipeline tick executions",
        );
        metrics::describe_counter!(
            "blackflower_world_prediction_reconciliations_total",
            Unit::Count,
            "Authoritative prediction reconciliation attempts",
        );
        metrics::describe_counter!(
            "blackflower_world_prediction_system_executions_total",
            Unit::Count,
            "Prediction system executions by phase and pass",
        );
        metrics::describe_histogram!(
            "blackflower_world_prediction_tick_duration_seconds",
            Unit::Seconds,
            "Wall-clock duration of a prediction pipeline tick",
        );
        metrics::describe_histogram!(
            "blackflower_world_prediction_reconciliation_duration_seconds",
            Unit::Seconds,
            "Wall-clock duration of authoritative prediction reconciliation",
        );
        metrics::describe_histogram!(
            "blackflower_world_prediction_resimulated_ticks",
            Unit::Count,
            "Prediction ticks re-simulated during one reconciliation",
        );
    }
}

#[cfg(any(feature = "metrics", feature = "tracing"))]
pub(crate) const fn pass_name(pass: PredictionPass) -> &'static str {
    match pass {
        PredictionPass::Forward => "forward",
        PredictionPass::Resimulation => "resimulation",
    }
}

#[cfg(any(feature = "metrics", feature = "tracing"))]
fn tick_outcome(result: &Result<bool, PredictionError>) -> &'static str {
    match result {
        Ok(true) => "completed",
        Ok(false) => "stopped",
        Err(_) => "failed",
    }
}

#[cfg(any(feature = "metrics", feature = "tracing"))]
fn reconciliation_result<E>(
    result: &Result<ReconciliationOutcome, ReconciliationError<E>>,
) -> (&'static str, &'static str)
where
    E: std::error::Error + 'static,
{
    match result {
        Ok(ReconciliationOutcome::Converged { .. }) => ("converged", "none"),
        Ok(ReconciliationOutcome::Reconciled { .. }) => ("reconciled", "state_mismatch"),
        Ok(ReconciliationOutcome::HardResyncRequired { reason }) => {
            ("hard_resync", hard_resync_reason(reason))
        }
        Err(ReconciliationError::Driver(_)) => ("failed", "driver"),
        Err(ReconciliationError::History(_)) => ("failed", "history"),
        Err(ReconciliationError::DriverTickMismatch { .. }) => ("failed", "driver_tick_mismatch"),
    }
}

#[cfg(any(feature = "metrics", feature = "tracing"))]
const fn hard_resync_reason(reason: &HardResyncReason) -> &'static str {
    match reason {
        HardResyncReason::SnapshotAhead { .. } => "snapshot_ahead",
        HardResyncReason::MissingPredictedState { .. } => "missing_predicted_state",
        HardResyncReason::ResimulationLimitExceeded { .. } => "resimulation_limit",
        HardResyncReason::MissingInput { .. } => "missing_input",
        HardResyncReason::TickOverflow => "tick_overflow",
    }
}

#[cfg(feature = "metrics")]
fn histogram_count(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
