use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sysinfo::{
    Components, CpuRefreshKind, DiskRefreshKind, Disks, MemoryRefreshKind, Networks, Pid,
    ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System,
};

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) struct HostMetricsCollector {
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl HostMetricsCollector {
    pub(crate) fn start() -> Option<Self> {
        if !sysinfo::IS_SUPPORTED_SYSTEM {
            tracing::warn!(
                target: "blackflower_observability",
                event_name = "host_metrics_unsupported",
                "host metrics unavailable",
            );
            return None;
        }

        let pid = match sysinfo::get_current_pid() {
            Ok(pid) => pid,
            Err(error) => {
                tracing::warn!(
                    target: "blackflower_observability",
                    event_name = "host_metrics_pid_unavailable",
                    error = %error,
                    "host metrics unavailable",
                );
                return None;
            }
        };
        Self::spawn(pid)
    }

    fn spawn(pid: Pid) -> Option<Self> {
        let (stop, receiver) = mpsc::channel();
        let worker = match std::thread::Builder::new()
            .name("blackflower-host-metrics".to_owned())
            .spawn(move || run_collector(pid, &receiver))
        {
            Ok(worker) => worker,
            Err(error) => {
                tracing::warn!(
                    target: "blackflower_observability",
                    event_name = "host_metrics_thread_unavailable",
                    error = %error,
                    "host metrics unavailable",
                );
                return None;
            }
        };
        Some(Self {
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for HostMetricsCollector {
    fn drop(&mut self) {
        if self.stop.send(()).is_err() {
            tracing::debug!(
                target: "blackflower_observability",
                event_name = "host_metrics_already_stopped",
                "host metrics stopped",
            );
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!(
                target: "blackflower_observability",
                event_name = "host_metrics_thread_failed",
                "host metrics failed",
            );
        }
    }
}

struct Sampler {
    system: System,
    disks: Disks,
    networks: Networks,
    components: Components,
    pid: Pid,
}

impl Sampler {
    fn new(pid: Pid) -> Self {
        let mut system = System::new_with_specifics(system_refresh_kind());
        refresh_process(&mut system, pid);
        Self {
            system,
            disks: Disks::new_with_refreshed_list_specifics(DiskRefreshKind::everything()),
            networks: Networks::new_with_refreshed_list(),
            components: Components::new_with_refreshed_list(),
            pid,
        }
    }

    fn refresh_and_publish(&mut self) {
        let started = Instant::now();
        self.system
            .refresh_cpu_specifics(CpuRefreshKind::everything());
        self.system.refresh_memory_specifics(memory_refresh_kind());
        refresh_process(&mut self.system, self.pid);
        self.disks.refresh(true);
        self.networks.refresh(true);
        self.components.refresh(true);

        self.publish_host();
        self.publish_process();
        metrics::gauge!("blackflower_observability_host_collection_duration_seconds")
            .set(started.elapsed().as_secs_f64());
        metrics::gauge!("blackflower_observability_host_collector_up").set(1.0);
    }

    fn publish_host(&self) {
        publish_time_and_load();
        publish_uname();
        self.publish_cpu();
        self.publish_memory();
        self.publish_disks();
        self.publish_networks();
        self.publish_components();
    }

    fn publish_cpu(&self) {
        let global_usage = usage_ratio(self.system.global_cpu_usage());
        metrics::gauge!("node_cpu_usage_ratio", "cpu" => "all").set(global_usage);
        for (index, cpu) in self.system.cpus().iter().enumerate() {
            let cpu_label = index.to_string();
            metrics::gauge!("node_cpu_usage_ratio", "cpu" => cpu_label.clone())
                .set(usage_ratio(cpu.cpu_usage()));
            metrics::gauge!("node_cpu_frequency_hertz", "cpu" => cpu_label.clone())
                .set(metric_f64(cpu.frequency().saturating_mul(1_000_000)));
            metrics::gauge!(
                "node_cpu_info",
                "cpu" => cpu_label,
                "vendor" => cpu.vendor_id().to_owned(),
                "model_name" => cpu.brand().to_owned(),
            )
            .set(1.0);
        }
    }

    fn publish_memory(&self) {
        set_bytes("node_memory_MemTotal_bytes", self.system.total_memory());
        set_bytes("node_memory_MemFree_bytes", self.system.free_memory());
        set_bytes(
            "node_memory_MemAvailable_bytes",
            self.system.available_memory(),
        );
        set_bytes("node_memory_SwapTotal_bytes", self.system.total_swap());
        set_bytes("node_memory_SwapFree_bytes", self.system.free_swap());
    }

    fn publish_disks(&self) {
        for disk in &self.disks {
            let device = disk.name().to_string_lossy().into_owned();
            let file_system = disk.file_system().to_string_lossy().into_owned();
            let mount_point = disk.mount_point().to_string_lossy().into_owned();
            let usage = disk.usage();

            metrics::gauge!(
                "node_filesystem_size_bytes",
                "device" => device.clone(),
                "fstype" => file_system.clone(),
                "mountpoint" => mount_point.clone(),
            )
            .set(metric_f64(disk.total_space()));
            metrics::gauge!(
                "node_filesystem_avail_bytes",
                "device" => device.clone(),
                "fstype" => file_system.clone(),
                "mountpoint" => mount_point.clone(),
            )
            .set(metric_f64(disk.available_space()));
            metrics::gauge!(
                "node_filesystem_readonly",
                "device" => device.clone(),
                "fstype" => file_system,
                "mountpoint" => mount_point,
            )
            .set(bool_value(disk.is_read_only()));
            metrics::counter!("node_disk_read_bytes_total", "device" => device.clone())
                .absolute(usage.total_read_bytes);
            metrics::counter!("node_disk_written_bytes_total", "device" => device)
                .absolute(usage.total_written_bytes);
        }
    }

    fn publish_networks(&self) {
        let mut networks = self.networks.iter().collect::<Vec<_>>();
        networks.sort_unstable_by(|left, right| left.0.cmp(right.0));
        for (device, network) in networks {
            publish_network_counters(device, network);
        }
    }

    fn publish_components(&self) {
        for component in &self.components {
            let chip = component.id().unwrap_or_else(|| component.label());
            let sensor = component.label();
            if let Some(temperature) = component.temperature().filter(|value| value.is_finite()) {
                metrics::gauge!(
                    "node_hwmon_temp_celsius",
                    "chip" => chip.to_owned(),
                    "sensor" => sensor.to_owned(),
                )
                .set(f64::from(temperature));
            }
            if let Some(critical) = component.critical().filter(|value| value.is_finite()) {
                metrics::gauge!(
                    "node_hwmon_temp_crit_celsius",
                    "chip" => chip.to_owned(),
                    "sensor" => sensor.to_owned(),
                )
                .set(f64::from(critical));
            }
        }
    }

    fn publish_process(&self) {
        let Some(process) = self.system.process(self.pid) else {
            return;
        };
        metrics::counter!("process_cpu_seconds_total")
            .absolute(process.accumulated_cpu_time() / 1_000);
        set_bytes("process_resident_memory_bytes", process.memory());
        set_bytes("process_virtual_memory_bytes", process.virtual_memory());
        metrics::gauge!("process_start_time_seconds").set(metric_f64(process.start_time()));
        if let Some(open_files) = process.open_files() {
            metrics::gauge!("process_open_fds").set(metric_usize(open_files));
        }
        if let Some(open_files_limit) = process.open_files_limit() {
            metrics::gauge!("process_max_fds").set(metric_usize(open_files_limit));
        }
    }
}

fn run_collector(pid: Pid, receiver: &Receiver<()>) {
    let mut sampler = Sampler::new(pid);
    sampler.refresh_and_publish();
    while let Err(RecvTimeoutError::Timeout) = receiver.recv_timeout(SAMPLE_INTERVAL) {
        sampler.refresh_and_publish();
    }
    metrics::gauge!("blackflower_observability_host_collector_up").set(0.0);
}

fn publish_time_and_load() {
    let load = System::load_average();
    metrics::gauge!("node_boot_time_seconds").set(metric_f64(System::boot_time()));
    metrics::gauge!("node_time_seconds").set(unix_time_seconds());
    metrics::gauge!("node_load1").set(load.one);
    metrics::gauge!("node_load5").set(load.five);
    metrics::gauge!("node_load15").set(load.fifteen);
}

fn publish_uname() {
    metrics::gauge!(
        "node_uname_info",
        "sysname" => System::name().unwrap_or_default(),
        "release" => System::kernel_version().unwrap_or_default(),
        "version" => System::os_version().unwrap_or_default(),
        "machine" => System::cpu_arch(),
        "nodename" => System::host_name().unwrap_or_default(),
        "domainname" => String::new(),
    )
    .set(1.0);
}

fn publish_network_counters(device: &str, network: &sysinfo::NetworkData) {
    metrics::counter!("node_network_receive_bytes_total", "device" => device.to_owned())
        .absolute(network.total_received());
    metrics::counter!("node_network_transmit_bytes_total", "device" => device.to_owned())
        .absolute(network.total_transmitted());
    metrics::counter!("node_network_receive_packets_total", "device" => device.to_owned())
        .absolute(network.total_packets_received());
    metrics::counter!("node_network_transmit_packets_total", "device" => device.to_owned())
        .absolute(network.total_packets_transmitted());
    metrics::counter!("node_network_receive_errs_total", "device" => device.to_owned())
        .absolute(network.total_errors_on_received());
    metrics::counter!("node_network_transmit_errs_total", "device" => device.to_owned())
        .absolute(network.total_errors_on_transmitted());
}

fn system_refresh_kind() -> RefreshKind {
    RefreshKind::nothing()
        .with_cpu(CpuRefreshKind::everything())
        .with_memory(memory_refresh_kind())
}

fn memory_refresh_kind() -> MemoryRefreshKind {
    MemoryRefreshKind::nothing().with_ram().with_swap()
}

fn process_refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cpu()
        .with_disk_usage()
        .with_memory()
        .without_tasks()
}

fn refresh_process(system: &mut System, pid: Pid) {
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        process_refresh_kind(),
    );
}

fn usage_ratio(percent: f32) -> f64 {
    (f64::from(percent) / 100.0).clamp(0.0, 1.0)
}

fn bool_value(value: bool) -> f64 {
    f64::from(u8::from(value))
}

fn unix_time_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

fn set_bytes(name: &'static str, value: u64) {
    metrics::gauge!(name).set(metric_f64(value));
}

fn metric_usize(value: usize) -> f64 {
    metric_f64(u64::try_from(value).unwrap_or(u64::MAX))
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Prometheus gauges use f64 values; byte counters exceed exact precision only at sizes where individual-byte precision is immaterial"
)]
fn metric_f64(value: u64) -> f64 {
    value as f64
}

pub(crate) fn describe_metrics() {
    describe_runtime_metrics();
    describe_cpu_metrics();
    describe_memory_metrics();
    describe_storage_metrics();
    describe_network_metrics();
    describe_temperature_metrics();
    describe_process_metrics();
    describe_collector_metrics();
}

fn describe_runtime_metrics() {
    describe_gauge(
        "node_boot_time_seconds",
        metrics::Unit::Seconds,
        "Host boot time since the Unix epoch",
    );
    describe_gauge(
        "node_time_seconds",
        metrics::Unit::Seconds,
        "Current host time since the Unix epoch",
    );
    describe_gauge(
        "node_load1",
        metrics::Unit::Count,
        "One-minute host load average",
    );
    describe_gauge(
        "node_load5",
        metrics::Unit::Count,
        "Five-minute host load average",
    );
    describe_gauge(
        "node_load15",
        metrics::Unit::Count,
        "Fifteen-minute host load average",
    );
    describe_gauge(
        "node_uname_info",
        metrics::Unit::Count,
        "Host operating-system identity information",
    );
}

fn describe_cpu_metrics() {
    describe_gauge(
        "node_cpu_usage_ratio",
        metrics::Unit::Count,
        "Current CPU utilization reported by sysinfo; cpu=all is host-wide",
    );
    describe_gauge(
        "node_cpu_frequency_hertz",
        metrics::Unit::Count,
        "Current logical CPU frequency",
    );
    describe_gauge(
        "node_cpu_info",
        metrics::Unit::Count,
        "Logical CPU identity information",
    );
}

fn describe_memory_metrics() {
    describe_gauge(
        "node_memory_MemTotal_bytes",
        metrics::Unit::Bytes,
        "Total host physical memory",
    );
    describe_gauge(
        "node_memory_MemFree_bytes",
        metrics::Unit::Bytes,
        "Unused host physical memory",
    );
    describe_gauge(
        "node_memory_MemAvailable_bytes",
        metrics::Unit::Bytes,
        "Host physical memory available without swapping",
    );
    describe_gauge(
        "node_memory_SwapTotal_bytes",
        metrics::Unit::Bytes,
        "Total host swap space",
    );
    describe_gauge(
        "node_memory_SwapFree_bytes",
        metrics::Unit::Bytes,
        "Unused host swap space",
    );
}

fn describe_storage_metrics() {
    describe_gauge(
        "node_filesystem_size_bytes",
        metrics::Unit::Bytes,
        "Filesystem size",
    );
    describe_gauge(
        "node_filesystem_avail_bytes",
        metrics::Unit::Bytes,
        "Filesystem space available to the process",
    );
    describe_gauge(
        "node_filesystem_readonly",
        metrics::Unit::Count,
        "Whether the filesystem is read-only",
    );
    describe_counter(
        "node_disk_read_bytes_total",
        metrics::Unit::Bytes,
        "Bytes read from the disk since boot",
    );
    describe_counter(
        "node_disk_written_bytes_total",
        metrics::Unit::Bytes,
        "Bytes written to the disk since boot",
    );
}

fn describe_network_metrics() {
    describe_counter(
        "node_network_receive_bytes_total",
        metrics::Unit::Bytes,
        "Network bytes received since boot",
    );
    describe_counter(
        "node_network_transmit_bytes_total",
        metrics::Unit::Bytes,
        "Network bytes transmitted since boot",
    );
    describe_counter(
        "node_network_receive_packets_total",
        metrics::Unit::Count,
        "Network packets received since boot",
    );
    describe_counter(
        "node_network_transmit_packets_total",
        metrics::Unit::Count,
        "Network packets transmitted since boot",
    );
    describe_counter(
        "node_network_receive_errs_total",
        metrics::Unit::Count,
        "Network receive errors since boot",
    );
    describe_counter(
        "node_network_transmit_errs_total",
        metrics::Unit::Count,
        "Network transmit errors since boot",
    );
}

fn describe_temperature_metrics() {
    describe_gauge(
        "node_hwmon_temp_celsius",
        metrics::Unit::Count,
        "Current hardware sensor temperature",
    );
    describe_gauge(
        "node_hwmon_temp_crit_celsius",
        metrics::Unit::Count,
        "Critical hardware sensor temperature",
    );
}

fn describe_process_metrics() {
    describe_counter(
        "process_cpu_seconds_total",
        metrics::Unit::Seconds,
        "Total CPU seconds consumed by the process",
    );
    describe_gauge(
        "process_resident_memory_bytes",
        metrics::Unit::Bytes,
        "Current process resident memory",
    );
    describe_gauge(
        "process_virtual_memory_bytes",
        metrics::Unit::Bytes,
        "Current process virtual memory",
    );
    describe_gauge(
        "process_start_time_seconds",
        metrics::Unit::Seconds,
        "Process start time since the Unix epoch",
    );
    describe_gauge(
        "process_open_fds",
        metrics::Unit::Count,
        "Current process open file descriptors",
    );
    describe_gauge(
        "process_max_fds",
        metrics::Unit::Count,
        "Maximum process open file descriptors",
    );
}

fn describe_collector_metrics() {
    describe_gauge(
        "blackflower_observability_host_collector_up",
        metrics::Unit::Count,
        "Whether the embedded sysinfo collector is running",
    );
    describe_gauge(
        "blackflower_observability_host_collection_duration_seconds",
        metrics::Unit::Seconds,
        "Duration of the latest embedded host collection",
    );
}

fn describe_gauge(name: &'static str, unit: metrics::Unit, description: &'static str) {
    metrics::describe_gauge!(name, unit, description);
}

fn describe_counter(name: &'static str, unit: metrics::Unit, description: &'static str) {
    metrics::describe_counter!(name, unit, description);
}

#[cfg(test)]
#[path = "../tests/unit/host_metrics.rs"]
mod tests;
