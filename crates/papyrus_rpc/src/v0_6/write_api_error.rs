use starknet_client::starknet_error::{KnownStarknetErrorCode, StarknetError, StarknetErrorCode};

use super::error::{
    unexpected_error,
    validation_failure,
    JsonRpcError,
    CLASS_ALREADY_DECLARED,
    CLASS_HASH_NOT_FOUND,
    COMPILATION_FAILED,
    COMPILED_CLASS_HASH_MISMATCH,
    CONTRACT_CLASS_SIZE_IS_TOO_LARGE,
    DUPLICATE_TX,
    INSUFFICIENT_ACCOUNT_BALANCE,
    INSUFFICIENT_MAX_FEE,
    INVALID_TRANSACTION_NONCE,
    NON_ACCOUNT,
    UNSUPPORTED_CONTRACT_CLASS_VERSION,
    UNSUPPORTED_TX_VERSION,
};

#[cfg(test)]
#[path = "write_api_error_test.rs"]
mod write_api_error_test;

pub(crate) fn starknet_error_to_invoke_error(error: StarknetError) -> JsonRpcError<String> {
    let StarknetErrorCode::KnownErrorCode(known_error_code) = error.code else {
        return unexpected_error(error.message);
    };
    match known_error_code {
        KnownStarknetErrorCode::DuplicatedTransaction => DUPLICATE_TX,
        // EntryPointNotFoundInContract is not thrown in __execute__ since that is
        // considered as a reverted transaction. It is also not thrown in __validate__ since
        // every error there is considered a ValidateFailure. This means that if
        // EntryPointNotFoundInContract is thrown then it failed because it couldn't find
        // __validate__ or __execute__ and that means the contract is not an account contract.
        KnownStarknetErrorCode::EntryPointNotFoundInContract => NON_ACCOUNT,
        KnownStarknetErrorCode::InsufficientAccountBalance => INSUFFICIENT_ACCOUNT_BALANCE,
        KnownStarknetErrorCode::InsufficientMaxFee => INSUFFICIENT_MAX_FEE,
        KnownStarknetErrorCode::InvalidTransactionNonce => INVALID_TRANSACTION_NONCE,
        KnownStarknetErrorCode::InvalidTransactionVersion => UNSUPPORTED_TX_VERSION,
        KnownStarknetErrorCode::ValidateFailure => validation_failure(error.message),
        _ => unexpected_error(error.message),
    }
}

pub(crate) fn starknet_error_to_declare_error(error: StarknetError) -> JsonRpcError<String> {
    let StarknetErrorCode::KnownErrorCode(known_error_code) = error.code else {
        return unexpected_error(error.message);
    };
    match known_error_code {
        KnownStarknetErrorCode::ClassAlreadyDeclared => CLASS_ALREADY_DECLARED,
        KnownStarknetErrorCode::CompilationFailed => COMPILATION_FAILED,
        KnownStarknetErrorCode::ContractBytecodeSizeTooLarge => CONTRACT_CLASS_SIZE_IS_TOO_LARGE,
        KnownStarknetErrorCode::ContractClassObjectSizeTooLarge => CONTRACT_CLASS_SIZE_IS_TOO_LARGE,
        KnownStarknetErrorCode::DuplicatedTransaction => DUPLICATE_TX,
        // See explanation on this mapping in AddInvokeError.
        KnownStarknetErrorCode::EntryPointNotFoundInContract => NON_ACCOUNT,
        KnownStarknetErrorCode::InsufficientAccountBalance => INSUFFICIENT_ACCOUNT_BALANCE,
        KnownStarknetErrorCode::InsufficientMaxFee => INSUFFICIENT_MAX_FEE,
        KnownStarknetErrorCode::InvalidCompiledClassHash => COMPILED_CLASS_HASH_MISMATCH,
        KnownStarknetErrorCode::InvalidContractClassVersion => UNSUPPORTED_CONTRACT_CLASS_VERSION,
        KnownStarknetErrorCode::InvalidTransactionNonce => INVALID_TRANSACTION_NONCE,
        KnownStarknetErrorCode::InvalidTransactionVersion => UNSUPPORTED_TX_VERSION,
        KnownStarknetErrorCode::ValidateFailure => validation_failure(error.message),
        _ => unexpected_error(error.message),
    }
}

pub(crate) fn starknet_error_to_deploy_account_error(error: StarknetError) -> JsonRpcError<String> {
    let StarknetErrorCode::KnownErrorCode(known_error_code) = error.code else {
        return unexpected_error(error.message);
    };
    match known_error_code {
        KnownStarknetErrorCode::DuplicatedTransaction => DUPLICATE_TX,
        // See explanation on this mapping in AddInvokeError.
        KnownStarknetErrorCode::EntryPointNotFoundInContract => NON_ACCOUNT,
        KnownStarknetErrorCode::InsufficientAccountBalance => INSUFFICIENT_ACCOUNT_BALANCE,
        KnownStarknetErrorCode::InsufficientMaxFee => INSUFFICIENT_MAX_FEE,
        KnownStarknetErrorCode::InvalidTransactionNonce => INVALID_TRANSACTION_NONCE,
        KnownStarknetErrorCode::InvalidTransactionVersion => UNSUPPORTED_TX_VERSION,
        KnownStarknetErrorCode::UndeclaredClass => CLASS_HASH_NOT_FOUND,
        KnownStarknetErrorCode::ValidateFailure => validation_failure(error.message),
        _ => unexpected_error(error.message),
    }
}
