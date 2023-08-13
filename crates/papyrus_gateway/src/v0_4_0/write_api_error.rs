#[cfg(test)]
#[path = "write_api_error_test.rs"]
mod write_api_error_test;

#[cfg(test)]
use mockall::automock;

pub(crate) use self::functions::{
    starknet_error_to_declare_error,
    starknet_error_to_deploy_account_error,
    starknet_error_to_invoke_error,
};
// We don't use mockall_double here because we don't want to use the mock functions in all the
// tests in this crate. Instead, we'll use mockall_double in the modules that use these
// functions and want to mock them in all of the module's tests.
#[cfg(test)]
pub(crate) use self::mock_functions::{
    starknet_error_to_declare_error as mock_starknet_error_to_declare_error,
    starknet_error_to_declare_error_context,
    starknet_error_to_deploy_account_error as mock_starknet_error_to_deploy_account_error,
    starknet_error_to_deploy_account_error_context,
    starknet_error_to_invoke_error as mock_starknet_error_to_invoke_error,
    starknet_error_to_invoke_error_context,
};

#[cfg_attr(test, automock())]
pub(crate) mod functions {
    use starknet_client::starknet_error::{
        KnownStarknetErrorCode,
        StarknetError,
        StarknetErrorCode,
    };

    use crate::v0_4_0::error::{
        unexpected_error,
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
        VALIDATION_FAILURE,
    };

    // TODO(shahak): Remove allow dead code once this function is used.
    #[allow(dead_code)]
    pub(crate) fn starknet_error_to_invoke_error(error: StarknetError) -> JsonRpcError {
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
            KnownStarknetErrorCode::ValidateFailure => VALIDATION_FAILURE,
            _ => unexpected_error(error.message),
        }
    }

    // TODO(shahak): Remove allow dead code once this function is used.
    #[allow(dead_code)]
    pub(crate) fn starknet_error_to_declare_error(error: StarknetError) -> JsonRpcError {
        let StarknetErrorCode::KnownErrorCode(known_error_code) = error.code else {
            return unexpected_error(error.message);
        };
        match known_error_code {
            KnownStarknetErrorCode::ClassAlreadyDeclared => CLASS_ALREADY_DECLARED,
            KnownStarknetErrorCode::CompilationFailed => COMPILATION_FAILED,
            KnownStarknetErrorCode::ContractBytecodeSizeTooLarge => {
                CONTRACT_CLASS_SIZE_IS_TOO_LARGE
            }
            KnownStarknetErrorCode::ContractClassObjectSizeTooLarge => {
                CONTRACT_CLASS_SIZE_IS_TOO_LARGE
            }
            KnownStarknetErrorCode::DuplicatedTransaction => DUPLICATE_TX,
            // See explanation on this mapping in AddInvokeError.
            KnownStarknetErrorCode::EntryPointNotFoundInContract => NON_ACCOUNT,
            KnownStarknetErrorCode::InsufficientAccountBalance => INSUFFICIENT_ACCOUNT_BALANCE,
            KnownStarknetErrorCode::InsufficientMaxFee => INSUFFICIENT_MAX_FEE,
            KnownStarknetErrorCode::InvalidCompiledClassHash => COMPILED_CLASS_HASH_MISMATCH,
            KnownStarknetErrorCode::InvalidContractClassVersion => {
                UNSUPPORTED_CONTRACT_CLASS_VERSION
            }
            KnownStarknetErrorCode::InvalidTransactionNonce => INVALID_TRANSACTION_NONCE,
            KnownStarknetErrorCode::InvalidTransactionVersion => UNSUPPORTED_TX_VERSION,
            KnownStarknetErrorCode::ValidateFailure => VALIDATION_FAILURE,
            _ => unexpected_error(error.message),
        }
    }

    // TODO(shahak): Remove allow dead code once this function is used.
    #[allow(dead_code)]
    pub(crate) fn starknet_error_to_deploy_account_error(error: StarknetError) -> JsonRpcError {
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
            KnownStarknetErrorCode::ValidateFailure => VALIDATION_FAILURE,
            _ => unexpected_error(error.message),
        }
    }
}
