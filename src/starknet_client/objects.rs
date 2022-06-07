#[derive(thiserror::Error, Debug)]
pub enum DeserializationError {
    #[error(transparent)]
    FromHexError(#[from] hex::FromHexError),
    #[error(
        "Bad input - expected #bytes: {expected_byte_count:?}, string found: {string_found:?}."
    )]
    BadInput {
        expected_byte_count: usize,
        string_found: String,
    },
}
#[allow(unused)]
pub fn bytes_from_hex_str<const N: usize>(hex_str: &str) -> Result<[u8; N], DeserializationError> {
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);

    // Make sure string is not too long.
    if hex_str.len() > 2 * N {
        let mut err_str = "0x".to_owned();
        err_str.push_str(hex_str);
        return Err(DeserializationError::BadInput {
            expected_byte_count: N,
            string_found: err_str,
        });
    }

    // Pad if needed.
    let to_add = 2 * N - hex_str.len();
    let padded_str = vec!["0"; to_add].join("") + hex_str;

    hex::decode(&padded_str)?
        .try_into()
        .map_err(|_| DeserializationError::BadInput {
            expected_byte_count: N,
            string_found: padded_str,
        })
}
