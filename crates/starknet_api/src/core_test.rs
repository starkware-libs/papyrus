use assert_matches::assert_matches;

use crate::core::{PatriciaKey, StarknetApiError};
use crate::hash::StarkHash;
use crate::shash;

#[test]
fn patricia_key_valid() {
    let hash = shash!("0x123");
    let patricia_key = PatriciaKey::try_from(hash).unwrap();
    assert_eq!(patricia_key.0, hash);
}

#[test]
fn patricia_key_out_of_range() {
    // 2**251
    let hash = shash!("0x800000000000000000000000000000000000000000000000000000000000000");
    let err = PatriciaKey::try_from(hash);
    assert_matches!(err, Err(StarknetApiError::OutOfRange { string: _err_str }));
}
