use super::{ReaderClientError, ReaderStarknetError, ReaderStarknetErrorCode};
use crate::starknet_error::{KnownStarknetErrorCode, StarknetError, StarknetErrorCode};
use crate::ClientError;

#[test]
fn starknet_error_to_starknet_reader_error_known_code_for_both() {
    const MESSAGE: &str = "message";
    for (known_starknet_error_code, expected_reader_starknet_error_code) in [
        (KnownStarknetErrorCode::BlockNotFound, ReaderStarknetErrorCode::BlockNotFound),
        (KnownStarknetErrorCode::MalformedRequest, ReaderStarknetErrorCode::MalformedRequest),
        (KnownStarknetErrorCode::OutOfRangeClassHash, ReaderStarknetErrorCode::OutOfRangeClassHash),
        (KnownStarknetErrorCode::UndeclaredClass, ReaderStarknetErrorCode::UndeclaredClass),
    ] {
        let client_error = ClientError::StarknetError(StarknetError {
            code: StarknetErrorCode::KnownErrorCode(known_starknet_error_code),
            message: MESSAGE.to_string(),
        });
        // TODO(shahak) Use assert_matches once ReaderClientError derives from Debug.
        match client_error.into() {
            ReaderClientError::StarknetError(ReaderStarknetError { code, message })
                if code == expected_reader_starknet_error_code && message == *MESSAGE => {}
            _ => panic!("Converted error did not match expected error"),
        };
    }
}

#[test]
fn starknet_error_to_starknet_reader_error_unknown_code_for_reader() {
    const MESSAGE: &str = "message";
    for (expected_error_code_string, known_starknet_error_code) in [
        ("StarknetErrorCode.CLASS_ALREADY_DECLARED", KnownStarknetErrorCode::ClassAlreadyDeclared),
        ("StarknetErrorCode.COMPILATION_FAILED", KnownStarknetErrorCode::CompilationFailed),
        (
            "StarknetErrorCode.CONTRACT_BYTECODE_SIZE_TOO_LARGE",
            KnownStarknetErrorCode::ContractBytecodeSizeTooLarge,
        ),
        (
            "StarknetErrorCode.CONTRACT_CLASS_OBJECT_SIZE_TOO_LARGE",
            KnownStarknetErrorCode::ContractClassObjectSizeTooLarge,
        ),
        ("StarknetErrorCode.DUPLICATED_TRANSACTION", KnownStarknetErrorCode::DuplicatedTransaction),
        (
            "StarknetErrorCode.ENTRY_POINT_NOT_FOUND_IN_CONTRACT",
            KnownStarknetErrorCode::EntryPointNotFoundInContract,
        ),
        (
            "StarknetErrorCode.INSUFFICIENT_ACCOUNT_BALANCE",
            KnownStarknetErrorCode::InsufficientAccountBalance,
        ),
        ("StarknetErrorCode.INSUFFICIENT_MAX_FEE", KnownStarknetErrorCode::InsufficientMaxFee),
        (
            "StarknetErrorCode.INVALID_COMPILED_CLASS_HASH",
            KnownStarknetErrorCode::InvalidCompiledClassHash,
        ),
        (
            "StarknetErrorCode.INVALID_CONTRACT_CLASS_VERSION",
            KnownStarknetErrorCode::InvalidContractClassVersion,
        ),
        (
            "StarknetErrorCode.INVALID_TRANSACTION_NONCE",
            KnownStarknetErrorCode::InvalidTransactionNonce,
        ),
        (
            "StarknetErrorCode.INVALID_TRANSACTION_VERSION",
            KnownStarknetErrorCode::InvalidTransactionVersion,
        ),
        ("StarknetErrorCode.VALIDATE_FAILURE", KnownStarknetErrorCode::ValidateFailure),
        (
            "StarknetErrorCode.TRANSACTION_LIMIT_EXCEEDED",
            KnownStarknetErrorCode::TransactionLimitExceeded,
        ),
    ] {
        let client_error = ClientError::StarknetError(StarknetError {
            code: StarknetErrorCode::KnownErrorCode(known_starknet_error_code),
            message: MESSAGE.to_string(),
        });
        // TODO(shahak) Use assert_matches once ReaderClientError derives from Debug.
        match client_error.into() {
            ReaderClientError::StarknetError(ReaderStarknetError {
                code: ReaderStarknetErrorCode::UnknownErrorCode(error_code_string),
                message,
            }) if error_code_string == expected_error_code_string && message == *MESSAGE => {}
            _ => panic!("Converted error did not match expected error"),
        };
    }
}

#[test]
fn starknet_error_to_starknet_reader_error_unknown_code_for_both() {
    const CODE_STR: &str = "StarknetErrorCode.MADE_UP_CODE_FOR_TEST";
    const MESSAGE: &str = "message";
    let client_error = ClientError::StarknetError(StarknetError {
        code: StarknetErrorCode::UnknownErrorCode(CODE_STR.to_string()),
        message: MESSAGE.to_string(),
    });
    // TODO(shahak) Use assert_matches once ReaderClientError derives from Debug.
    match client_error.into() {
        ReaderClientError::StarknetError(ReaderStarknetError {
            code: ReaderStarknetErrorCode::UnknownErrorCode(error_code_string),
            message,
        }) if error_code_string == *CODE_STR && message == *MESSAGE => {}
        _ => panic!("Converted error did not match expected error"),
    };
}
