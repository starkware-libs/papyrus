use crate::{shash, StarkHash};

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
fn test_hash_serde() {
    let hash = shash!("0x123");
    assert_eq!(hash, serde_json::from_str(&serde_json::to_string(&hash).unwrap()).unwrap());
}
