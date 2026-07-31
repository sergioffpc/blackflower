use super::*;

fn descriptor() -> PropagationDescriptor {
    PropagationDescriptor {
        structure_version: AcousticStructureVersion(2),
        arrival_sample: 480,
        path_length_mm: 3_430,
        gain_db_q8: -6 * 256,
        band_gain: BandEnergy([u16::MAX, 40_000, 20_000]),
        direction_q15: [1, 2, 3],
        uncertainty_q16: 4,
        direct: true,
    }
}

#[test]
fn effects_and_exchange_need_no_per_frame_storage() -> Result<(), Error> {
    let propagation = descriptor();
    let exchange = PropagationExchange::new(propagation);
    exchange.publish(propagation);
    assert_eq!(exchange.latest(), propagation);
    let input = [0.25_f32; 8];
    let mut scratch = [0.0_f32; 8];
    let mut output = [0.0_f32; 8];
    DirectEffect::new(8)?.process(propagation, &input, &mut scratch)?;
    PathEffect::new(8)?.process(propagation, &scratch, &mut output)?;
    assert!(output.iter().any(|sample| *sample != 0.0));
    Ok(())
}
