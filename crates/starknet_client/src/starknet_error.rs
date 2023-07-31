/// Error codes returned by the starknet gateway.
#[derive(Copy, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum StarknetErrorCode {
    #[serde(rename = "StarknetErrorCode.BLOCK_NOT_FOUND")]
    BlockNotFound = 0,
    #[serde(rename = "StarknetErrorCode.OUT_OF_RANGE_CLASS_HASH")]
    OutOfRangeClassHash = 26,
    #[serde(rename = "StarkErrorCode.MALFORMED_REQUEST")]
    MalformedRequest = 32,
    #[serde(rename = "StarknetErrorCode.UNDECLARED_CLASS")]
    UndeclaredClass = 44,
}

/// A client error wrapping error codes returned by the starknet gateway.
#[derive(thiserror::Error, Debug, Deserialize, Serialize)]
pub struct StarknetError {
    pub code: StarknetErrorCode,
    pub message: String,
}

impl Display for StarknetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
