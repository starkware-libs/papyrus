use assert_matches::assert_matches;

use crate::serde_utils::DeserializationError;
use crate::{shash, PatriciaKey, StarkHash};

#[test]
fn test_hash_macro() {
    assert_eq!(
        shash!("0x123"),
        StarkHash::new([
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0x1, 0x23
        ])
        .unwrap()
    );
}

#[test]
fn test_hash_serde() {
    let hash = shash!("0x123");
    assert_eq!(hash, serde_json::from_str(&serde_json::to_string(&hash).unwrap()).unwrap());
}

#[test]
fn test_valid_patricia_key() {
    let hash = shash!("0x123");
    let patricia_key = PatriciaKey::new(hash).unwrap();
    assert_eq!(patricia_key.into_hash(), hash);
}

#[test]
fn test_out_of_range_patricia_key() {
    // 2**251
    let hash = shash!("0x800000000000000000000000000000000000000000000000000000000000000");
    let err = PatriciaKey::new(hash);
    assert_matches!(err, Err(DeserializationError::OutOfRange { string: _err_str }));
}
