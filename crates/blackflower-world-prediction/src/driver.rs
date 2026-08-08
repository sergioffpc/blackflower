use crate::PredictionPass;

/// Prediction-owned bridge used for both forward execution and reconciliation.
///
/// Implementations restore the complete predicted subset from an authoritative
/// snapshot and execute one gameplay entry point for both prediction passes.
/// This prevents separate handwritten forward and re-simulation paths without
/// claiming that client and server floating-point results are bit-identical.
pub trait PredictionDriver<State, Input> {
    /// Concrete prediction-world or gameplay failure.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Return the latest tick represented by the prediction state.
    fn current_tick(&self) -> u64;

    /// Restore the complete predicted state to one authoritative baseline.
    fn restore_authoritative(&mut self, tick: u64, state: &State) -> Result<(), Self::Error>;

    /// Evaluate one forward or re-simulation tick and return its sealed state.
    fn simulate_tick(
        &mut self,
        pass: PredictionPass,
        tick: u64,
        input: &Input,
    ) -> Result<State, Self::Error>;
}
