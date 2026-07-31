use core::str::FromStr;

use super::ContentHash;

#[test]
fn hashes_round_trip_as_lowercase_hex() -> Result<(), Box<dyn std::error::Error>> {
    let hash = ContentHash::hash_bytes(b"blackflower");
    let encoded = hash.to_string();
    assert_eq!(encoded.len(), 64);
    assert_eq!(ContentHash::from_str(&encoded)?, hash);
    Ok(())
}

#[test]
fn rejects_uppercase_and_wrong_length() {
    assert!(ContentHash::from_str("AB").is_err());
    assert!(ContentHash::from_str(&"0".repeat(63)).is_err());
}
