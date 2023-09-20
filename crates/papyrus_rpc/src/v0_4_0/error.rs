use jsonrpsee::types::ErrorObjectOwned;

#[derive(Clone, Debug)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: &'static str,
    pub data: Option<String>,
}

// TODO(yair): Remove allow(dead_code) once all errors are used.
#[allow(dead_code)]
pub const FAILED_TO_RECEIVE_TRANSACTION: JsonRpcError =
    JsonRpcError { code: 1, message: "Failed to write transaction", data: None };

pub const CONTRACT_NOT_FOUND: JsonRpcError =
    JsonRpcError { code: 20, message: "Contract not found", data: None };

pub const INVALID_TRANSACTION_HASH: JsonRpcError =
    JsonRpcError { code: 25, message: "Invalid transaction hash", data: None };

pub const INVALID_BLOCK_HASH: JsonRpcError =
    JsonRpcError { code: 26, message: "Invalid block hash", data: None };

pub const BLOCK_NOT_FOUND: JsonRpcError =
    JsonRpcError { code: 24, message: "Block not found", data: None };

pub const INVALID_TRANSACTION_INDEX: JsonRpcError =
    JsonRpcError { code: 27, message: "Invalid transaction index in a block", data: None };

pub const CLASS_HASH_NOT_FOUND: JsonRpcError =
    JsonRpcError { code: 28, message: "Class hash not found", data: None };

pub const TRANSACTION_HASH_NOT_FOUND: JsonRpcError =
    JsonRpcError { code: 29, message: "Transaction hash not found", data: None };

pub const PAGE_SIZE_TOO_BIG: JsonRpcError =
    JsonRpcError { code: 31, message: "Requested page size is too big", data: None };

pub const NO_BLOCKS: JsonRpcError =
    JsonRpcError { code: 32, message: "There are no blocks", data: None };

pub const INVALID_CONTINUATION_TOKEN: JsonRpcError = JsonRpcError {
    code: 33,
    message: "The supplied continuation token is invalid or unknown",
    data: None,
};

pub const TOO_MANY_KEYS_IN_FILTER: JsonRpcError =
    JsonRpcError { code: 34, message: "Too many keys provided in a filter", data: None };

pub const CONTRACT_ERROR: JsonRpcError =
    JsonRpcError { code: 40, message: "Contract error", data: None };

pub const CLASS_ALREADY_DECLARED: JsonRpcError =
    JsonRpcError { code: 51, message: "Class already declared", data: None };

pub const INVALID_TRANSACTION_NONCE: JsonRpcError =
    JsonRpcError { code: 52, message: "Invalid transaction nonce", data: None };

pub const INSUFFICIENT_MAX_FEE: JsonRpcError = JsonRpcError {
    code: 53,
    message: "Max fee is smaller than the minimal transaction cost (validation plus fee transfer)",
    data: None,
};

pub const INSUFFICIENT_ACCOUNT_BALANCE: JsonRpcError = JsonRpcError {
    code: 54,
    message: "Account balance is smaller than the transaction's max_fee",
    data: None,
};

pub const VALIDATION_FAILURE: JsonRpcError =
    JsonRpcError { code: 55, message: "Account validation failed", data: None };

pub const COMPILATION_FAILED: JsonRpcError =
    JsonRpcError { code: 56, message: "Compilation failed", data: None };

pub const CONTRACT_CLASS_SIZE_IS_TOO_LARGE: JsonRpcError =
    JsonRpcError { code: 57, message: "Contract class size it too large", data: None };

pub const NON_ACCOUNT: JsonRpcError =
    JsonRpcError { code: 58, message: "Sender address in not an account contract", data: None };

pub const DUPLICATE_TX: JsonRpcError = JsonRpcError {
    code: 59,
    message: "A transaction with the same hash already exists in the mempool",
    data: None,
};

pub const COMPILED_CLASS_HASH_MISMATCH: JsonRpcError = JsonRpcError {
    code: 60,
    message: "the compiled class hash did not match the one supplied in the transaction",
    data: None,
};

pub const UNSUPPORTED_TX_VERSION: JsonRpcError =
    JsonRpcError { code: 61, message: "the transaction version is not supported", data: None };

pub const UNSUPPORTED_CONTRACT_CLASS_VERSION: JsonRpcError =
    JsonRpcError { code: 62, message: "the contract class version is not supported", data: None };

pub fn unexpected_error(data: String) -> JsonRpcError {
    JsonRpcError { code: 63, message: "An unexpected error occurred", data: Some(data) }
}

impl From<JsonRpcError> for ErrorObjectOwned {
    fn from(err: JsonRpcError) -> Self {
        ErrorObjectOwned::owned(err.code, err.message, err.data)
    }
}
