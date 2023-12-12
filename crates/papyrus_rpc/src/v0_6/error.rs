use jsonrpsee::types::ErrorObjectOwned;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct JsonRpcError<T: Serialize> {
    pub code: i32,
    pub message: &'static str,
    pub data: Option<T>,
}

// TODO(yair): Remove allow(dead_code) once all errors are used.
#[allow(dead_code)]
pub const FAILED_TO_RECEIVE_TRANSACTION: JsonRpcError<String> =
    JsonRpcError { code: 1, message: "Failed to write transaction", data: None };

pub const CONTRACT_NOT_FOUND: JsonRpcError<String> =
    JsonRpcError { code: 20, message: "Contract not found", data: None };

pub const INVALID_TRANSACTION_HASH: JsonRpcError<String> =
    JsonRpcError { code: 25, message: "Invalid transaction hash", data: None };

// TODO(shahak): Remove allow(dead_code) once all errors are used.
#[allow(dead_code)]
pub const INVALID_BLOCK_HASH: JsonRpcError<String> =
    JsonRpcError { code: 26, message: "Invalid block hash", data: None };

pub const BLOCK_NOT_FOUND: JsonRpcError<String> =
    JsonRpcError { code: 24, message: "Block not found", data: None };

pub const INVALID_TRANSACTION_INDEX: JsonRpcError<String> =
    JsonRpcError { code: 27, message: "Invalid transaction index in a block", data: None };

pub const CLASS_HASH_NOT_FOUND: JsonRpcError<String> =
    JsonRpcError { code: 28, message: "Class hash not found", data: None };

pub const TRANSACTION_HASH_NOT_FOUND: JsonRpcError<String> =
    JsonRpcError { code: 29, message: "Transaction hash not found", data: None };

pub const PAGE_SIZE_TOO_BIG: JsonRpcError<String> =
    JsonRpcError { code: 31, message: "Requested page size is too big", data: None };

pub const NO_BLOCKS: JsonRpcError<String> =
    JsonRpcError { code: 32, message: "There are no blocks", data: None };

pub const INVALID_CONTINUATION_TOKEN: JsonRpcError<String> = JsonRpcError {
    code: 33,
    message: "The supplied continuation token is invalid or unknown",
    data: None,
};

pub const TOO_MANY_KEYS_IN_FILTER: JsonRpcError<String> =
    JsonRpcError { code: 34, message: "Too many keys provided in a filter", data: None };

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
pub struct ContractError {
    pub revert_error: String,
}

impl From<ContractError> for JsonRpcError<ContractError> {
    fn from(contract_error: ContractError) -> Self {
        Self { code: 40, message: "Contract error", data: Some(contract_error) }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
pub struct TransactionExecutionError {
    pub transaction_index: usize,
    pub execution_error: String,
}

impl From<TransactionExecutionError> for JsonRpcError<TransactionExecutionError> {
    fn from(tx_execution_error: TransactionExecutionError) -> Self {
        Self { code: 41, message: "Transaction execution error", data: Some(tx_execution_error) }
    }
}
pub const CLASS_ALREADY_DECLARED: JsonRpcError<String> =
    JsonRpcError { code: 51, message: "Class already declared", data: None };

pub const INVALID_TRANSACTION_NONCE: JsonRpcError<String> =
    JsonRpcError { code: 52, message: "Invalid transaction nonce", data: None };

pub const INSUFFICIENT_MAX_FEE: JsonRpcError<String> = JsonRpcError {
    code: 53,
    message: "Max fee is smaller than the minimal transaction cost (validation plus fee transfer)",
    data: None,
};

pub const INSUFFICIENT_ACCOUNT_BALANCE: JsonRpcError<String> = JsonRpcError {
    code: 54,
    message: "Account balance is smaller than the transaction's max_fee",
    data: None,
};

pub fn validation_failure(data: String) -> JsonRpcError<String> {
    JsonRpcError { code: 55, message: "Account validation failed", data: Some(data) }
}

pub const COMPILATION_FAILED: JsonRpcError<String> =
    JsonRpcError { code: 56, message: "Compilation failed", data: None };

pub const CONTRACT_CLASS_SIZE_IS_TOO_LARGE: JsonRpcError<String> =
    JsonRpcError { code: 57, message: "Contract class size it too large", data: None };

pub const NON_ACCOUNT: JsonRpcError<String> =
    JsonRpcError { code: 58, message: "Sender address in not an account contract", data: None };

pub const DUPLICATE_TX: JsonRpcError<String> = JsonRpcError {
    code: 59,
    message: "A transaction with the same hash already exists in the mempool",
    data: None,
};

pub const COMPILED_CLASS_HASH_MISMATCH: JsonRpcError<String> = JsonRpcError {
    code: 60,
    message: "the compiled class hash did not match the one supplied in the transaction",
    data: None,
};

pub const UNSUPPORTED_TX_VERSION: JsonRpcError<String> =
    JsonRpcError { code: 61, message: "the transaction version is not supported", data: None };

pub const UNSUPPORTED_CONTRACT_CLASS_VERSION: JsonRpcError<String> =
    JsonRpcError { code: 62, message: "the contract class version is not supported", data: None };

pub fn unexpected_error(data: String) -> JsonRpcError<String> {
    JsonRpcError { code: 63, message: "An unexpected error occurred", data: Some(data) }
}

impl<T: Serialize> From<JsonRpcError<T>> for ErrorObjectOwned {
    fn from(err: JsonRpcError<T>) -> Self {
        ErrorObjectOwned::owned(err.code, err.message, err.data)
    }
}
