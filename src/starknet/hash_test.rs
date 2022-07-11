use crate::starknet::StarkHash;
use crate::test_utils::serde_utils::run_serde_test;

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
fn test_hash_serde() {
    run_serde_test(&shash!("0x123"));
}
