use blackflower_world_prediction::{
    AbsoluteTolerance, AngularTolerance, PredictionStateComparison, ToleranceError,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn absolute_tolerance_accepts_submillimetre_position_error() -> TestResult {
    let position_tolerance = AbsoluteTolerance::new(0.001)?;

    assert_eq!(
        position_tolerance.compare(12.0, 12.000_9),
        PredictionStateComparison::WithinTolerance
    );
    assert_eq!(
        position_tolerance.compare(12.0, 12.001_1),
        PredictionStateComparison::CorrectionRequired
    );
    Ok(())
}

#[test]
fn non_finite_values_always_require_correction() -> TestResult {
    let tolerance = AbsoluteTolerance::new(1.0)?;

    assert_eq!(
        tolerance.compare(f64::NAN, f64::NAN),
        PredictionStateComparison::CorrectionRequired
    );
    assert_eq!(
        tolerance.compare(f64::INFINITY, f64::INFINITY),
        PredictionStateComparison::CorrectionRequired
    );
    Ok(())
}

#[test]
fn angular_tolerance_uses_the_shortest_arc_across_the_turn_boundary() -> TestResult {
    let tolerance = AngularTolerance::new(0.01)?;
    let predicted = std::f64::consts::PI - 0.002;
    let authoritative = -std::f64::consts::PI + 0.002;

    assert_eq!(
        tolerance.compare(predicted, authoritative),
        PredictionStateComparison::WithinTolerance
    );
    Ok(())
}

#[test]
fn invalid_tolerances_are_rejected() {
    assert_eq!(
        AbsoluteTolerance::new(-0.001),
        Err(ToleranceError::InvalidAbsoluteTolerance)
    );
    assert_eq!(
        AngularTolerance::new(std::f64::consts::TAU),
        Err(ToleranceError::InvalidAngularTolerance)
    );
}
