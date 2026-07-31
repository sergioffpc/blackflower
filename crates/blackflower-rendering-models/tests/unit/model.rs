use bytes::Bytes;

use super::{ModelAsset, ModelNode, NodeTransform, encode_model};

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

#[test]
fn round_trips_matrix_only_nodes() -> Result<(), crate::Error> {
    let nodes = vec![
        ModelNode::new(
            Some("Root".to_owned()),
            None,
            NodeTransform::matrix(IDENTITY)?,
        )?,
        ModelNode::new(
            None,
            Some(0),
            NodeTransform::matrix([
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 4.0, 5.0, 6.0, 1.0,
            ])?,
        )?,
    ];

    let encoded = encode_model(&nodes, &[])?;
    assert_eq!(&encoded[8..12], 1_u32.to_le_bytes().as_slice());
    let decoded = ModelAsset::from_bytes(encoded.clone())?;
    assert_eq!(decoded.bytes(), &encoded);
    assert_eq!(decoded.nodes(), nodes);
    assert_eq!(
        decoded.nodes()[1].transform().to_cols_array()[12..15],
        [4.0, 5.0, 6.0]
    );
    Ok(())
}

#[test]
fn rejects_unreleased_version_two_payloads() -> Result<(), crate::Error> {
    let node = ModelNode::new(None, None, NodeTransform::matrix(IDENTITY)?)?;
    let mut bytes = encode_model(&[node], &[])?.to_vec();
    bytes[8..12].copy_from_slice(&2_u32.to_le_bytes());
    assert!(ModelAsset::from_bytes(Bytes::from(bytes)).is_err());
    Ok(())
}

#[test]
fn rejects_non_finite_matrices() {
    let mut matrix = IDENTITY;
    matrix[0] = f32::NAN;
    assert!(NodeTransform::matrix(matrix).is_err());
}
