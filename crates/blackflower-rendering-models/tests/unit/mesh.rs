use bytes::Bytes;

use super::{MeshAsset, MeshLod, MeshPrimitive, MeshVertex, VertexAttributes, encode_mesh};

fn vertex(x: f32, y: f32) -> MeshVertex {
    MeshVertex {
        position: [x, y, 0.0],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0, 1.0],
        texcoord_0: [x, y],
    }
}

#[test]
fn round_trips_a_lod_chain() -> Result<(), crate::Error> {
    let base = MeshLod::new(
        0.0,
        vec![
            vertex(0.0, 0.0),
            vertex(1.0, 0.0),
            vertex(1.0, 1.0),
            vertex(0.0, 1.0),
        ],
        vec![0, 1, 2, 0, 2, 3],
    )?;
    let coarse = MeshLod::new(
        0.25,
        vec![vertex(0.0, 0.0), vertex(1.0, 0.0), vertex(0.0, 1.0)],
        vec![0, 1, 2],
    )?;
    let primitive = MeshPrimitive::new(
        Some(3),
        VertexAttributes::positions()
            .with(VertexAttributes::NORMAL)
            .with(VertexAttributes::TEXCOORD_0),
        vec![base, coarse],
    )?;
    let encoded = encode_mesh(std::slice::from_ref(&primitive))?;
    let decoded = MeshAsset::from_bytes(encoded.clone())?;
    assert_eq!(decoded.bytes(), &encoded);
    assert_eq!(decoded.primitives(), &[primitive]);
    Ok(())
}

#[test]
fn rejects_trailing_bytes() -> Result<(), crate::Error> {
    let lod = MeshLod::new(
        0.0,
        vec![vertex(0.0, 0.0), vertex(1.0, 0.0), vertex(0.0, 1.0)],
        vec![0, 1, 2],
    )?;
    let primitive = MeshPrimitive::new(None, VertexAttributes::positions(), vec![lod])?;
    let mut bytes = encode_mesh(&[primitive])?.to_vec();
    bytes.push(0);
    assert!(MeshAsset::from_bytes(Bytes::from(bytes)).is_err());
    Ok(())
}

#[test]
fn rejects_unreleased_version_two_payloads() -> Result<(), crate::Error> {
    let lod = MeshLod::new(
        0.0,
        vec![vertex(0.0, 0.0), vertex(1.0, 0.0), vertex(0.0, 1.0)],
        vec![0, 1, 2],
    )?;
    let primitive = MeshPrimitive::new(None, VertexAttributes::positions(), vec![lod])?;
    let mut bytes = encode_mesh(&[primitive])?.to_vec();
    bytes[8..12].copy_from_slice(&2_u32.to_le_bytes());
    assert!(MeshAsset::from_bytes(Bytes::from(bytes)).is_err());
    Ok(())
}
