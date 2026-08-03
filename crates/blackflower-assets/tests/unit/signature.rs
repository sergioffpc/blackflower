use ed25519_dalek::{
    SigningKey,
    pkcs8::{EncodePrivateKey, EncodePublicKey, spki::der::pem::LineEnding},
};

use super::{AssetSigningKey, AssetTrustStore};

#[test]
fn standard_private_and_public_pem_formats_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let signing_key = SigningKey::from_bytes(&[0x42; 32]);
    let private_pem = signing_key.to_pkcs8_pem(LineEnding::LF)?;
    let decoded = AssetSigningKey::from_pkcs8_pem(&private_pem)?;
    assert_eq!(
        decoded.public_key_bytes(),
        signing_key.verifying_key().to_bytes()
    );

    let public_pem = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)?;
    let mut trust_store = AssetTrustStore::new();
    let _key_id = trust_store.trust_public_key_pem(&public_pem)?;
    assert_eq!(trust_store.len(), 1);
    Ok(())
}
