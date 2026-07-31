use super::SimulationTick;

#[test]
fn simulation_tick_advances_until_its_representation_is_exhausted() {
    assert_eq!(
        SimulationTick::ZERO.checked_next(),
        Some(SimulationTick::new(1))
    );
    assert_eq!(SimulationTick::new(u64::MAX).checked_next(), None);
}
