use std::error::Error;
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use blackflower_observability::{ObservabilityConfig, init};

#[test]
fn server_exposes_embedded_host_metrics() -> Result<(), Box<dyn Error>> {
    let address = unused_loopback_address()?;
    let observability = init(
        &ObservabilityConfig::server("blackflower-observability-test", "0.1.0")
            .with_metrics_bind_address(Some(address)),
    )?;

    assert!(observability.prometheus_listener_active());
    assert!(observability.host_metrics_active());

    let exposition = wait_for_host_metrics(address)?;
    for metric in [
        "blackflower_observability_host_collector_up 1",
        "node_boot_time_seconds ",
        "node_cpu_usage_ratio{cpu=\"all\"}",
        "node_memory_MemTotal_bytes ",
        "node_filesystem_size_bytes{",
        "process_resident_memory_bytes ",
    ] {
        assert!(
            exposition.contains(metric),
            "Prometheus exposition did not contain {metric:?}",
        );
    }
    Ok(())
}

fn unused_loopback_address() -> Result<SocketAddr, Box<dyn Error>> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    Ok(listener.local_addr()?)
}

fn wait_for_host_metrics(address: SocketAddr) -> Result<String, Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(exposition) = scrape(address)
            && exposition.contains("blackflower_observability_host_collector_up 1")
        {
            return Ok(exposition);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err("timed out waiting for embedded host metrics".into())
}

fn scrape(address: SocketAddr) -> Result<String, Box<dyn Error>> {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(250))?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}
