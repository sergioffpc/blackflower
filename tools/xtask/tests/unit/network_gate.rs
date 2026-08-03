use super::*;

#[test]
fn report_simulation_is_deterministic_and_inside_each_profile() -> anyhow::Result<()> {
    for profile in [
        GateProfile::Smoke,
        GateProfile::Nominal,
        GateProfile::Degraded,
    ] {
        let specification = specification(profile);
        let first = simulate_with_pacing(specification, 17, false)?;
        let second = simulate_with_pacing(specification, 17, false)?;
        assert_eq!(first.packets_attempted, second.packets_attempted);
        assert_eq!(first.packets_lost, second.packets_lost);
        assert_eq!(first.p99_rtt_millis, second.p99_rtt_millis);
        assert_eq!(first.packets_invalid, 0);
        assert!(first.p99_rtt_millis <= specification.thresholds.p99_rtt_millis);
        assert!(first.maximum_jitter_millis <= specification.thresholds.jitter_millis);
        assert!(first.loss_basis_points <= specification.thresholds.loss_basis_points);
    }
    Ok(())
}
