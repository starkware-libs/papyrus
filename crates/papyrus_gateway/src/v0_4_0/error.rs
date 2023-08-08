use jsonrpsee::types::ErrorObjectOwned;

#[derive(Clone, Debug)]
pub enum JsonRpcError {
    FailedToReceiveTransaction,
    ContractNotFound,
    InvalidTransactionHash,
    InvalidBlockHash,
    BlockNotFound,
    InvalidTransactionIndex,
    ClassHashNotFound,
    TransactionHashNotFound,
    PageSizeTooBig,
    NoBlocks,
    InvalidContinuationToken,
    TooManyKeysInFilter,
    ContractError,
    // TODO(dvir): delete this when start support pending blocks.
    PendingBlocksNotSupported,
    ClassAlreadyDeclared,
    InvalidTransactionNonce,
    InsufficientMaxFee,
    InsufficientAccountBalance,
    ValidationFailure,
    CompilationFailed,
    ContractClassSizeIsTooLarge,
    NonAccount,
    DuplicateTx,
    CompiledClassHashMismatch,
    UnsupportedTxVersion,
    UnsupportedContractClassVersion,
    UnexpectedError(String),
}

impl From<JsonRpcError> for ErrorObjectOwned {
    fn from(err: JsonRpcError) -> Self {
        match err {
            JsonRpcError::FailedToReceiveTransaction => {
                ErrorObjectOwned::owned(1, "Failed to write transaction", None::<()>)
            }
            JsonRpcError::ContractNotFound => {
                ErrorObjectOwned::owned(20, "Contract not found", None::<()>)
            }
            JsonRpcError::InvalidTransactionHash => {
                ErrorObjectOwned::owned(25, "Invalid transaction hash", None::<()>)
            }
            JsonRpcError::InvalidBlockHash => {
                ErrorObjectOwned::owned(26, "Invalid block hash", None::<()>)
            }
            JsonRpcError::BlockNotFound => {
                ErrorObjectOwned::owned(24, "Block not found", None::<()>)
            }
            JsonRpcError::InvalidTransactionIndex => {
                ErrorObjectOwned::owned(27, "Invalid transaction index in a block", None::<()>)
            }
            JsonRpcError::ClassHashNotFound => {
                ErrorObjectOwned::owned(28, "Class hash not found", None::<()>)
            }
            JsonRpcError::TransactionHashNotFound => {
                ErrorObjectOwned::owned(29, "Transaction hash not found", None::<()>)
            }
            JsonRpcError::PageSizeTooBig => {
                ErrorObjectOwned::owned(31, "Requested page size is too big", None::<()>)
            }
            JsonRpcError::NoBlocks => {
                ErrorObjectOwned::owned(32, "There are no blocks", None::<()>)
            }
            JsonRpcError::InvalidContinuationToken => ErrorObjectOwned::owned(
                33,
                "The supplied continuation token is invalid or unknown",
                None::<()>,
            ),
            JsonRpcError::TooManyKeysInFilter => {
                ErrorObjectOwned::owned(34, "Too many keys provided in a filter", None::<()>)
            }
            JsonRpcError::ContractError => {
                ErrorObjectOwned::owned(40, "Contract error", None::<()>)
            }
            JsonRpcError::PendingBlocksNotSupported => ErrorObjectOwned::owned(
                41,
                "Currently, Papyrus doesn't support pending blocks.",
                None::<()>,
            ),
            JsonRpcError::ClassAlreadyDeclared => {
                ErrorObjectOwned::owned(51, "Class already declared", None::<()>)
            }
            JsonRpcError::InvalidTransactionNonce => {
                ErrorObjectOwned::owned(52, "Invalid transaction nonce", None::<()>)
            }
            JsonRpcError::InsufficientMaxFee => ErrorObjectOwned::owned(
                53,
                "Max fee is smaller than the minimal transaction cost (validation plus fee \
                 transfer)",
                None::<()>,
            ),
            JsonRpcError::InsufficientAccountBalance => ErrorObjectOwned::owned(
                54,
                "Account balance is smaller than the transaction's max_fee",
                None::<()>,
            ),
            JsonRpcError::ValidationFailure => {
                ErrorObjectOwned::owned(55, "Account validation failed", None::<()>)
            }
            JsonRpcError::CompilationFailed => {
                ErrorObjectOwned::owned(56, "Compilation failed", None::<()>)
            }
            JsonRpcError::ContractClassSizeIsTooLarge => {
                ErrorObjectOwned::owned(57, "Contract class size it too large", None::<()>)
            }
            JsonRpcError::NonAccount => {
                ErrorObjectOwned::owned(58, "Sender address in not an account contract", None::<()>)
            }
            JsonRpcError::DuplicateTx => ErrorObjectOwned::owned(
                59,
                "A transaction with the same hash already exists in the mempool",
                None::<()>,
            ),
            JsonRpcError::CompiledClassHashMismatch => ErrorObjectOwned::owned(
                60,
                "the compiled class hash did not match the one supplied in the transaction",
                None::<()>,
            ),
            JsonRpcError::UnsupportedTxVersion => {
                ErrorObjectOwned::owned(61, "the transaction version is not supported", None::<()>)
            }
            JsonRpcError::UnsupportedContractClassVersion => ErrorObjectOwned::owned(
                62,
                "the contract class version is not supported",
                None::<()>,
            ),
            JsonRpcError::UnexpectedError(data) => {
                ErrorObjectOwned::owned(63, "An unexpected error occured", Some(data))
            }
        }
    }
}
