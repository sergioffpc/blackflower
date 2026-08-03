use std::cmp::Ordering;
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const SCRAPE_INTERVAL: Duration = Duration::from_secs(1);
const HTTP_TIMEOUT: Duration = Duration::from_millis(750);
const RESULT_QUEUE_CAPACITY: usize = 2;
const MAX_HTTP_RESPONSE_BYTES: usize = 8 * 1_024 * 1_024;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sample {
    pub(crate) name: String,
    pub(crate) labels: Vec<(String, String)>,
    pub(crate) value: f64,
}

impl Sample {
    pub(crate) fn label(&self, name: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|(candidate, _value)| candidate == name)
            .map(|(_name, value)| value.as_str())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MetricSnapshot {
    pub(crate) collected_at: Instant,
    samples: Vec<Sample>,
}

impl MetricSnapshot {
    #[must_use]
    pub(crate) fn value(&self, name: &str) -> Option<f64> {
        self.samples
            .iter()
            .find(|sample| sample.name == name && sample.labels.is_empty())
            .map(|sample| sample.value)
    }

    #[must_use]
    pub(crate) fn sum(&self, name: &str) -> Option<f64> {
        let mut found = false;
        let sum = self
            .samples
            .iter()
            .filter(|sample| sample.name == name)
            .fold(0.0, |total, sample| {
                found = true;
                total + sample.value
            });
        found.then_some(sum)
    }

    #[must_use]
    pub(crate) fn value_with_label(&self, name: &str, label: &str, value: &str) -> Option<f64> {
        self.samples
            .iter()
            .find(|sample| sample.name == name && sample.label(label) == Some(value))
            .map(|sample| sample.value)
    }

    #[must_use]
    pub(crate) fn series(&self, name: &str) -> Vec<&Sample> {
        self.samples
            .iter()
            .filter(|sample| sample.name == name)
            .collect()
    }

    #[must_use]
    pub(crate) fn histogram_quantile(&self, name: &str, quantile: f64) -> Option<f64> {
        let bucket_name = format!("{name}_bucket");
        let mut buckets: Vec<(String, f64, f64)> = Vec::new();
        for sample in self
            .samples
            .iter()
            .filter(|sample| sample.name == bucket_name)
        {
            let Some(upper_text) = sample.label("le") else {
                continue;
            };
            let Ok(upper) = upper_text.parse::<f64>() else {
                continue;
            };
            if let Some(existing) = buckets
                .iter_mut()
                .find(|(text, _upper, _count)| text == upper_text)
            {
                existing.2 += sample.value;
            } else {
                buckets.push((upper_text.to_owned(), upper, sample.value));
            }
        }
        buckets.sort_by(|left, right| left.1.partial_cmp(&right.1).unwrap_or(Ordering::Equal));
        let count = self.sum(&format!("{name}_count")).or_else(|| {
            buckets
                .last()
                .map(|(_text, _upper, cumulative_count)| *cumulative_count)
        })?;
        if count <= 0.0 || buckets.is_empty() {
            return None;
        }

        let rank = quantile.clamp(0.0, 1.0) * count;
        let mut previous_upper = 0.0;
        let mut previous_count = 0.0;
        for (_text, upper, cumulative_count) in buckets {
            if cumulative_count >= rank {
                if !upper.is_finite() {
                    return Some(previous_upper);
                }
                let bucket_count = cumulative_count - previous_count;
                if bucket_count <= 0.0 {
                    return Some(upper);
                }
                let fraction = (rank - previous_count) / bucket_count;
                return Some(previous_upper + (upper - previous_upper) * fraction);
            }
            previous_upper = upper;
            previous_count = cumulative_count;
        }
        None
    }
}

#[derive(Debug, Default)]
pub(crate) struct MetricStore {
    pub(crate) current: Option<MetricSnapshot>,
    previous: Option<MetricSnapshot>,
    pub(crate) last_success: Option<Instant>,
    pub(crate) last_attempt: Option<Instant>,
    pub(crate) last_error: Option<String>,
}

impl MetricStore {
    pub(crate) fn accept(&mut self, result: ScrapeResult) -> bool {
        self.last_attempt = Some(result.completed_at);
        match result.result {
            Ok(snapshot) => {
                self.previous = self.current.replace(snapshot);
                self.last_success = Some(result.completed_at);
                self.last_error = None;
                true
            }
            Err(error) => {
                self.last_error = Some(error);
                false
            }
        }
    }

    #[must_use]
    pub(crate) fn value(&self, name: &str) -> Option<f64> {
        self.current.as_ref()?.value(name)
    }

    #[must_use]
    pub(crate) fn sum(&self, name: &str) -> Option<f64> {
        self.current.as_ref()?.sum(name)
    }

    #[must_use]
    pub(crate) fn value_with_label(&self, name: &str, label: &str, value: &str) -> Option<f64> {
        self.current.as_ref()?.value_with_label(name, label, value)
    }

    #[must_use]
    pub(crate) fn histogram_quantile(&self, name: &str, quantile: f64) -> Option<f64> {
        self.current.as_ref()?.histogram_quantile(name, quantile)
    }

    #[must_use]
    pub(crate) fn rate(&self, name: &str) -> Option<f64> {
        let current = self.current.as_ref()?;
        let previous = self.previous.as_ref()?;
        counter_rate(current.sum(name)?, previous.sum(name)?, current, previous)
    }

    #[must_use]
    pub(crate) fn rate_with_label(&self, name: &str, label: &str, value: &str) -> Option<f64> {
        let current = self.current.as_ref()?;
        let previous = self.previous.as_ref()?;
        counter_rate(
            current.value_with_label(name, label, value)?,
            previous.value_with_label(name, label, value)?,
            current,
            previous,
        )
    }

    #[must_use]
    pub(crate) fn series(&self, name: &str) -> Vec<&Sample> {
        self.current
            .as_ref()
            .map_or_else(Vec::new, |snapshot| snapshot.series(name))
    }

    #[must_use]
    pub(crate) fn scrape_age(&self, now: Instant) -> Option<Duration> {
        self.last_success
            .map(|success| now.saturating_duration_since(success))
    }
}

fn counter_rate(
    current_value: f64,
    previous_value: f64,
    current: &MetricSnapshot,
    previous: &MetricSnapshot,
) -> Option<f64> {
    let elapsed = current
        .collected_at
        .saturating_duration_since(previous.collected_at)
        .as_secs_f64();
    if elapsed <= 0.0 || current_value < previous_value {
        return None;
    }
    Some((current_value - previous_value) / elapsed)
}

#[derive(Debug)]
pub(crate) struct ScrapeResult {
    pub(crate) completed_at: Instant,
    pub(crate) result: Result<MetricSnapshot, String>,
}

pub(crate) struct MetricsPoller {
    results: Receiver<ScrapeResult>,
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl MetricsPoller {
    pub(crate) fn start(address: SocketAddr) -> std::io::Result<Self> {
        let (result_sender, results) = mpsc::sync_channel(RESULT_QUEUE_CAPACITY);
        let (stop, stop_receiver) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("blackflower-foreground-metrics".to_owned())
            .spawn(move || poll_metrics(address, &result_sender, &stop_receiver))?;
        Ok(Self {
            results,
            stop,
            worker: Some(worker),
        })
    }

    pub(crate) fn try_recv(&self) -> Result<ScrapeResult, mpsc::TryRecvError> {
        self.results.try_recv()
    }
}

impl Drop for MetricsPoller {
    fn drop(&mut self) {
        if self.stop.send(()).is_err() {
            return;
        }
        if let Some(worker) = self.worker.take() {
            let _join_result = worker.join();
        }
    }
}

fn poll_metrics(address: SocketAddr, sender: &SyncSender<ScrapeResult>, stop: &Receiver<()>) {
    loop {
        let completed_at = Instant::now();
        let result = scrape(address)
            .and_then(|body| parse_prometheus(&body).map_err(std::io::Error::other))
            .map(|samples| MetricSnapshot {
                collected_at: completed_at,
                samples,
            })
            .map_err(|error| error.to_string());
        match sender.try_send(ScrapeResult {
            completed_at,
            result,
        }) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => return,
        }
        match stop.recv_timeout(SCRAPE_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn scrape(address: SocketAddr) -> std::io::Result<String> {
    let mut stream = TcpStream::connect_timeout(&address, HTTP_TIMEOUT)?;
    stream.set_read_timeout(Some(HTTP_TIMEOUT))?;
    stream.set_write_timeout(Some(HTTP_TIMEOUT))?;
    write!(
        stream,
        "GET /metrics HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAccept: text/plain\r\n\r\n"
    )?;
    let response = read_http_response(&mut stream)?;
    let response = String::from_utf8(response)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| std::io::Error::other("metrics response has no HTTP header boundary"))?;
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(std::io::Error::other(format!(
            "metrics endpoint returned {status}"
        )));
    }
    Ok(body.to_owned())
}

fn read_http_response(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut response = Vec::with_capacity(16 * 1_024);
    let mut buffer = [0_u8; 8 * 1_024];
    let mut expected_length = None;
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(response);
        }
        response.extend_from_slice(&buffer[..read]);
        if response.len() > MAX_HTTP_RESPONSE_BYTES {
            return Err(std::io::Error::other(
                "metrics response exceeds foreground limit",
            ));
        }
        if expected_length.is_none()
            && let Some(header_end) = find_header_end(&response)
        {
            expected_length = content_length(&response[..header_end])?
                .and_then(|body_length| header_end.checked_add(4 + body_length));
        }
        if expected_length.is_some_and(|expected| response.len() >= expected) {
            return Ok(response);
        }
    }
}

fn find_header_end(response: &[u8]) -> Option<usize> {
    response.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(headers: &[u8]) -> std::io::Result<Option<usize>> {
    let headers = std::str::from_utf8(headers)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    for line in headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            return value
                .trim()
                .parse::<usize>()
                .map(Some)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error));
        }
    }
    Ok(None)
}

pub(crate) fn parse_prometheus(body: &str) -> Result<Vec<Sample>, String> {
    let mut samples = Vec::new();
    for (line_index, line) in body.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (series, remainder) =
            split_sample(line).ok_or_else(|| format!("line {} has no value", line_index + 1))?;
        let value_text = remainder
            .split_whitespace()
            .next()
            .ok_or_else(|| format!("line {} has no value", line_index + 1))?;
        let value = value_text
            .parse::<f64>()
            .map_err(|error| format!("line {} has invalid value: {error}", line_index + 1))?;
        let (name, labels) = parse_series(series)
            .map_err(|error| format!("line {} has invalid series: {error}", line_index + 1))?;
        samples.push(Sample {
            name,
            labels,
            value,
        });
    }
    samples.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.labels.cmp(&right.labels))
    });
    Ok(samples)
}

fn split_sample(line: &str) -> Option<(&str, &str)> {
    let mut in_labels = false;
    let mut in_quotes = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_quotes && character == '\\' {
            escaped = true;
        } else if character == '"' && in_labels {
            in_quotes = !in_quotes;
        } else if character == '{' && !in_quotes {
            in_labels = true;
        } else if character == '}' && !in_quotes {
            in_labels = false;
        } else if character.is_whitespace() && !in_labels {
            return Some((&line[..index], &line[index..]));
        }
    }
    None
}

fn parse_series(series: &str) -> Result<(String, Vec<(String, String)>), String> {
    let Some(open) = series.find('{') else {
        return Ok((series.to_owned(), Vec::new()));
    };
    if !series.ends_with('}') {
        return Err("label set is not closed".to_owned());
    }
    let name = series[..open].to_owned();
    let labels = parse_labels(&series[open + 1..series.len() - 1])?;
    Ok((name, labels))
}

fn parse_labels(source: &str) -> Result<Vec<(String, String)>, String> {
    let mut labels = Vec::new();
    let mut remaining = source.trim();
    while !remaining.is_empty() {
        let equal = remaining
            .find('=')
            .ok_or_else(|| "label has no equals sign".to_owned())?;
        let name = remaining[..equal].trim();
        let after_equal = remaining[equal + 1..].trim_start();
        let (value, consumed) = parse_quoted(after_equal)?;
        labels.push((name.to_owned(), value));
        remaining = after_equal[consumed..].trim_start();
        if remaining.is_empty() {
            break;
        }
        remaining = remaining
            .strip_prefix(',')
            .ok_or_else(|| "labels are not comma separated".to_owned())?
            .trim_start();
    }
    labels.sort();
    Ok(labels)
}

fn parse_quoted(source: &str) -> Result<(String, usize), String> {
    if !source.starts_with('"') {
        return Err("label value is not quoted".to_owned());
    }
    let mut value = String::new();
    let mut escaped = false;
    for (offset, character) in source[1..].char_indices() {
        if escaped {
            match character {
                'n' => value.push('\n'),
                '\\' => value.push('\\'),
                '"' => value.push('"'),
                other => value.push(other),
            }
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Ok((value, offset + 2));
        } else {
            value.push(character);
        }
    }
    Err("label value is not closed".to_owned())
}

#[cfg(test)]
#[path = "../../tests/unit/foreground_metrics.rs"]
mod tests;
