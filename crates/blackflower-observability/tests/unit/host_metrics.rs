use super::{bool_value, metric_usize, usage_ratio};

#[test]
fn cpu_usage_is_normalized_to_a_ratio() {
    assert!((usage_ratio(42.5) - 0.425).abs() < f64::EPSILON);
    assert!((usage_ratio(150.0) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn boolean_metrics_use_prometheus_values() {
    assert!((bool_value(false) - 0.0).abs() < f64::EPSILON);
    assert!((bool_value(true) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn usize_metrics_remain_non_negative() {
    assert!(metric_usize(usize::MAX).is_sign_positive());
}
