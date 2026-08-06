use std::time::Duration;

pub(super) fn initialize() {
    describe_metrics();
    metrics::gauge!("blackflower_server_simulation_scheduler_catch_up_depth_ticks").set(0.0);
    metrics::counter!("blackflower_server_simulation_scheduler_catch_up_ticks_total").increment(0);
}

pub(super) fn tick_started(waited: Duration, lag: Duration, ticks_behind: u64) {
    metrics::histogram!("blackflower_server_simulation_scheduler_wait_seconds")
        .record(waited.as_secs_f64());
    metrics::histogram!("blackflower_server_simulation_scheduler_tick_lag_seconds")
        .record(lag.as_secs_f64());
    metrics::gauge!("blackflower_server_simulation_scheduler_catch_up_depth_ticks")
        .set(metric_u32(ticks_behind));
    if ticks_behind > 0 {
        metrics::counter!("blackflower_server_simulation_scheduler_catch_up_ticks_total")
            .increment(1);
    }
}

pub(super) fn tick_finished(deadline_pressure_ratio: f64) {
    metrics::histogram!("blackflower_server_simulation_scheduler_deadline_pressure_ratio")
        .record(deadline_pressure_ratio);
}

fn describe_metrics() {
    metrics::describe_histogram!(
        "blackflower_server_simulation_scheduler_wait_seconds",
        metrics::Unit::Seconds,
        "Time deliberately spent waiting for an authoritative tick deadline",
    );
    metrics::describe_histogram!(
        "blackflower_server_simulation_scheduler_tick_lag_seconds",
        metrics::Unit::Seconds,
        "Authoritative tick start lag behind its scheduled deadline",
    );
    metrics::describe_gauge!(
        "blackflower_server_simulation_scheduler_catch_up_depth_ticks",
        metrics::Unit::Count,
        "Current whole authoritative tick intervals behind schedule",
    );
    metrics::describe_counter!(
        "blackflower_server_simulation_scheduler_catch_up_ticks_total",
        metrics::Unit::Count,
        "Authoritative ticks started at least one whole interval behind schedule",
    );
    metrics::describe_histogram!(
        "blackflower_server_simulation_scheduler_deadline_pressure_ratio",
        metrics::Unit::Count,
        "Tick completion time since its scheduled start divided by the fixed-step budget",
    );
}

fn metric_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
