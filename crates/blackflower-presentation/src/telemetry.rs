use blackflower_ecs::TickDelta;

use crate::PresentationError;

pub(crate) struct FrameObservation {
    #[cfg(feature = "metrics")]
    started: std::time::Instant,
}

impl FrameObservation {
    pub(crate) fn start(delta: TickDelta) -> Self {
        #[cfg(not(feature = "metrics"))]
        let _ = delta;

        #[cfg(feature = "metrics")]
        metrics::histogram!("blackflower_presentation_frame_delta_seconds")
            .record(f64::from(delta.as_seconds()));

        Self {
            #[cfg(feature = "metrics")]
            started: std::time::Instant::now(),
        }
    }

    pub(crate) fn finish(self, _result: &Result<bool, PresentationError>) {
        #[cfg(any(feature = "metrics", feature = "tracing"))]
        let outcome = frame_outcome(_result);

        #[cfg(feature = "metrics")]
        {
            metrics::counter!(
                "blackflower_presentation_frames_total",
                "result" => outcome,
            )
            .increment(1);
            metrics::histogram!("blackflower_presentation_frame_duration_seconds")
                .record(self.started.elapsed().as_secs_f64());
        }

        #[cfg(feature = "tracing")]
        tracing::Span::current().record("result", outcome);
    }
}

pub(crate) fn frame_rejected(reason: &'static str) {
    #[cfg(feature = "metrics")]
    metrics::counter!(
        "blackflower_presentation_frames_total",
        "result" => "rejected",
    )
    .increment(1);

    #[cfg(feature = "tracing")]
    {
        let span = tracing::Span::current();
        span.record("result", "rejected");
        span.record("reason", reason);
        tracing::error!(
            target: "blackflower_presentation",
            reason,
            "frame rejected",
        );
    }

    #[cfg(not(any(feature = "metrics", feature = "tracing")))]
    let _ = reason;
}

pub(crate) fn describe_metrics() {
    #[cfg(feature = "metrics")]
    {
        use metrics::Unit;

        metrics::describe_counter!(
            "blackflower_presentation_frames_total",
            Unit::Count,
            "Client presentation frame executions",
        );
        metrics::describe_histogram!(
            "blackflower_presentation_frame_duration_seconds",
            Unit::Seconds,
            "Wall-clock duration of a client presentation frame",
        );
        metrics::describe_histogram!(
            "blackflower_presentation_frame_delta_seconds",
            Unit::Seconds,
            "Variable frame delta supplied to client presentation",
        );
    }
}

#[cfg(any(feature = "metrics", feature = "tracing"))]
fn frame_outcome(result: &Result<bool, PresentationError>) -> &'static str {
    match result {
        Ok(true) => "completed",
        Ok(false) => "stopped",
        Err(_) => "failed",
    }
}
