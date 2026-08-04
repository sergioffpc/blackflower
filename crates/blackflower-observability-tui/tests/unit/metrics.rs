use super::parse_prometheus;

#[test]
fn parses_gauges_labels_and_escaped_values() -> Result<(), String> {
    let samples = parse_prometheus(
        "# TYPE test gauge\n\
         test 3\n\
         network_bytes_total{device=\"en0\",note=\"Apple M2 \\\"Max\\\"\"} 42\n",
    )?;

    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].name, "network_bytes_total");
    assert_eq!(
        samples[0].labels[0],
        ("device".to_owned(), "en0".to_owned())
    );
    assert_eq!(
        samples[0].labels[1],
        ("note".to_owned(), "Apple M2 \"Max\"".to_owned())
    );
    assert_eq!(samples[1].value.to_bits(), 3.0_f64.to_bits());
    Ok(())
}

#[test]
fn calculates_histogram_quantiles() -> Result<(), String> {
    let samples = parse_prometheus(
        "tick_bucket{le=\"0.001\"} 5\n\
         tick_bucket{le=\"0.002\"} 9\n\
         tick_bucket{le=\"+Inf\"} 10\n\
         tick_count 10\n",
    )?;
    let snapshot = super::MetricSnapshot {
        collected_at: std::time::Instant::now(),
        samples,
    };

    assert_eq!(snapshot.histogram_quantile("tick", 0.5), Some(0.001));
    assert_eq!(snapshot.histogram_quantile("tick", 0.99), Some(0.002));
    Ok(())
}

#[test]
fn reads_content_length_case_insensitively() -> std::io::Result<()> {
    assert_eq!(
        super::content_length(b"HTTP/1.1 200 OK\r\nContent-Length: 17434")?,
        Some(17_434),
    );
    Ok(())
}
