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
