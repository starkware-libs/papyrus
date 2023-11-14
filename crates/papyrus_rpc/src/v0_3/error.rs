use jsonrpsee::types::ErrorObjectOwned;

#[derive(thiserror::Error, Clone, Copy, Debug)]
pub enum JsonRpcError {
    #[error("Contract not found")]
    ContractNotFound = 20,
    #[error("Block not found")]
    BlockNotFound = 24,
    #[error("Transaction hash not found")]
    TransactionHashNotFound = 25,
    #[error("Invalid transaction index in a block")]
    InvalidTransactionIndex = 27,
    #[error("Class hash not found")]
    ClassHashNotFound = 28,
    #[error("Transaction reverted")]
    TransactionReverted = 29,
    #[error("Requested page size is too big")]
    PageSizeTooBig = 31,
    #[error("There are no blocks")]
    NoBlocks = 32,
    #[error("The supplied continuation token is invalid or unknown")]
    InvalidContinuationToken = 33,
    #[error("Too many keys provided in a filter")]
    TooManyKeysInFilter = 34,
    #[error("Contract error")]
    ContractError = 40,
}

impl From<JsonRpcError> for ErrorObjectOwned {
    fn from(err: JsonRpcError) -> Self {
        ErrorObjectOwned::owned(err as i32, err.to_string(), None::<()>)
    }
}
