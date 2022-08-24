use assert_matches::assert_matches;

use super::{bytes_from_hex_str, hex_str_from_bytes, DeserializationError, HexAsBytes};
use crate::serde_utils::CONTRACT_ADRESS_UPPER_BOUND;
use crate::{shash, ContractAddress, StarkHash};

#[test]
fn test_hex_str_from_bytes() {
    // even length.
    assert_eq!(hex_str_from_bytes::<1, true>([106]), "0x6a");

    // odd length.
    assert_eq!(hex_str_from_bytes::<1, true>([6]), "0x6");

    // Remove padding.
    assert_eq!(hex_str_from_bytes::<2, true>([0, 6]), "0x6");

    // Non-prefixed.
    assert_eq!(hex_str_from_bytes::<2, false>([13, 162]), "da2");
}

#[test]
fn test_hex_str_from_bytes_zero() {
    // Prefixed.
    assert_eq!(hex_str_from_bytes::<3, true>([0, 0, 0]), "0x0");

    // Non-prefixed.
    assert_eq!(hex_str_from_bytes::<2, false>([0, 0]), "0");
}

#[test]
fn test_bytes_from_hex_str() {
    // even length.
    let hex_str = "0x6a";
    let res = bytes_from_hex_str::<1, true>(hex_str).unwrap();
    assert_eq!(res, [106]);

    // odd length.
    let hex_str = "0x6";
    let res = bytes_from_hex_str::<1, true>(hex_str).unwrap();
    assert_eq!(res, [6]);

    // No prefix.
    let hex_str = "6";
    let res = bytes_from_hex_str::<1, false>(hex_str).unwrap();
    assert_eq!(res, [6]);
}

#[test]
fn test_bytes_from_hex_str_padding() {
    // even length.
    let hex_str = "0xda2b";
    let res = bytes_from_hex_str::<4, true>(hex_str).unwrap();
    assert_eq!(res, [0, 0, 218, 43]);

    // odd length.
    let hex_str = "0xda2";
    let res = bytes_from_hex_str::<4, true>(hex_str).unwrap();
    assert_eq!(res, [0, 0, 13, 162]);
}

#[test]
fn test_bytes_from_hex_str_errors() {
    // Short buffer.
    let hex_str = "0xda2b";
    let err = bytes_from_hex_str::<1, true>(hex_str);
    assert_matches!(err, Err(DeserializationError::BadInput { expected_byte_count: 1, .. }));

    // Invalid hex char.
    let err = bytes_from_hex_str::<1, false>("1z");
    assert_matches!(
        err,
        Err(DeserializationError::FromHexError(hex::FromHexError::InvalidHexCharacter {
            c: 'z',
            index: 1
        }))
    );

    // Missing prefix.
    let err = bytes_from_hex_str::<2, true>("11");
    assert_matches!(err, Err(DeserializationError::MissingPrefix { .. }));

    // Unneeded prefix.
    let err = bytes_from_hex_str::<2, false>("0x11");
    assert_matches!(
        err,
        Err(DeserializationError::FromHexError(hex::FromHexError::InvalidHexCharacter {
            c: 'x',
            index: 1
        }))
    );
}

#[test]
fn test_hex_as_bytes_serde_prefixed() {
    let hex_as_bytes = HexAsBytes::<3, true>([1, 2, 3]);
    assert_eq!(
        hex_as_bytes,
        serde_json::from_str(&serde_json::to_string(&hex_as_bytes).unwrap()).unwrap()
    );
}

#[test]
fn test_hex_as_bytes_serde_not_prefixed() {
    let hex_as_bytes = HexAsBytes::<3, false>([1, 2, 3]);
    assert_eq!(
        hex_as_bytes,
        serde_json::from_str(&serde_json::to_string(&hex_as_bytes).unwrap()).unwrap()
    );
}

#[test]
fn test_desirialize_contract_address() {
    let expected = ContractAddress(shash!(
        "0x6324f76f396c5e1d79d2637cc714842c864b2cc732e164717819c77885bddd6"
    ));
    let serialized = &serde_json::to_string(&expected.0).unwrap();
    let deserialized = serde_json::from_str::<ContractAddress>(serialized).unwrap();
    assert_eq!(expected, deserialized);

    let not_in_range = ContractAddress(shash!("0x0"));
    let serialized = &serde_json::to_string(&not_in_range.0).unwrap();
    let deserialized = serde_json::from_str::<ContractAddress>(serialized);
    assert!(deserialized.is_err());

    let not_in_range = ContractAddress(shash!(CONTRACT_ADRESS_UPPER_BOUND));
    let serialized = &serde_json::to_string(&not_in_range.0).unwrap();
    let deserialized = serde_json::from_str::<ContractAddress>(serialized);
    assert!(deserialized.is_err());
}
