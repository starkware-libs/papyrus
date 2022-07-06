use crate::starknet::StarkHash;

use super::shash;

#[test]
fn test_hash_macro() {
    assert_eq!(
        shash!("0x123"),
        StarkHash([
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0x1, 0x23
        ])
    );
}

#[test]
fn test_serde_json() {
    let hash = shash!("0x123");
    let loaded_hash = serde_json::from_str(&serde_json::to_string(&hash).unwrap()).unwrap();
    assert_eq!(hash, loaded_hash);
}
