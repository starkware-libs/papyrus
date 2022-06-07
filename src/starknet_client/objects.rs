#[derive(thiserror::Error, Debug, PartialEq)]
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
    let mut hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);

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
    let mut str_with_leading_0s: String;
    if hex_str.len() / 2 < N {
        str_with_leading_0s = vec!["0"; to_add].join("");
        str_with_leading_0s.push_str(hex_str);
        hex_str = &str_with_leading_0s;
    }

    match hex::decode(hex_str)?.try_into() {
        Ok(arr) => Ok(arr),
        Err(_) => Err(DeserializationError::BadInput {
            expected_byte_count: N,
            string_found: hex_str.to_string(),
        }),
    }
}
