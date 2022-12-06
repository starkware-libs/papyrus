use assert_matches::assert_matches;

use crate::core::{ChainId, PatriciaKey, StarknetApiError};
use crate::hash::StarkHash;
use crate::{patky, shash};

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

#[test]
fn patricia_key_macro() {
    assert_eq!(
        patky!("0x123"),
        PatriciaKey::try_from(
            StarkHash::new([
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0x1, 0x23
            ])
            .unwrap()
        )
        .unwrap()
    );
}

#[test]
fn chain_id() {
    ChainId::try_from("AB").expect("AB should be valid ChainId");
    ChainId::try_from("A_B").expect("A_B should be valid ChainId");
    assert_matches!(
        ChainId::try_from(""),
        Err(StarknetApiError::InvalidChainId { invalid_chain_id: _invalid_chain_id })
    );
    assert_matches!(
        ChainId::try_from("a"),
        Err(StarknetApiError::InvalidChainId { invalid_chain_id: _invalid_chain_id })
    );
    assert_matches!(
        ChainId::try_from("A B"),
        Err(StarknetApiError::InvalidChainId { invalid_chain_id: _invalid_chain_id })
    );
    assert_matches!(
        ChainId::try_from("_"),
        Err(StarknetApiError::InvalidChainId { invalid_chain_id: _invalid_chain_id })
    );
}
