use crate::starknet_client::objects::{bytes_from_hex_str, DeserializationError};

#[test]
fn test_bytes_from_hex_str() {
    // even length.
    let hex_str = "0x6a";
    let res = bytes_from_hex_str::<1>(hex_str).unwrap();
    assert_eq!(res, [106]);

    // odd length.
    let hex_str = "0x6";
    let res = bytes_from_hex_str::<1>(hex_str).unwrap();
    assert_eq!(res, [6]);
}

#[test]
fn test_bytes_from_hex_str_padding() {
    // even length.
    let hex_str = "0xda2b";
    let res = bytes_from_hex_str::<4>(hex_str).unwrap();
    assert_eq!(res, [0, 0, 218, 43]);

    // odd length.
    let hex_str = "0xda2";
    let res = bytes_from_hex_str::<4>(hex_str).unwrap();
    assert_eq!(res, [0, 0, 13, 162]);
}

#[test]
fn test_bytes_from_hex_str_errors() {
    // Short buffer.
    let hex_str = "0xda2b";
    let err = bytes_from_hex_str::<1>(hex_str);
    assert_eq!(
        err,
        Err(DeserializationError::BadInput {
            expected_byte_count: (1),
            string_found: (hex_str.to_owned())
        })
    );

    // Invalid hex char.
    let err = bytes_from_hex_str::<1>("1z");
    assert_eq!(
        err,
        Err(DeserializationError::FromHexError(
            hex::FromHexError::InvalidHexCharacter { c: 'z', index: 1 }
        ))
    );
}
