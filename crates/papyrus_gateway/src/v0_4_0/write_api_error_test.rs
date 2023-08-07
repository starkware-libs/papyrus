use serde::Serialize;
use starknet_client::starknet_error::{KnownStarknetErrorCode, StarknetError, StarknetErrorCode};

use super::{
    AddDeclareError, AddDeployAccountError, AddInvokeError, ClassAlreadyDeclared,
    ClassHashNotFound, CompilationFailed, CompiledClassHashMismatch, ContractClassSizeIsTooLarge,
    DuplicateTx, ErrorCode, ErrorCodeWithData, InsufficientAccountBalance, InsufficientMaxFee,
    InvalidTransactionNonce, NonAccount, UnexpectedError, UnsupportedContractClassVersion,
    UnsupportedTxVersion, ValidationFailure,
};
use crate::test_utils::{get_starknet_spec_api_schema_for_method_errors, SpecFile};
use crate::version_config::VERSION_0_4;

fn test_error_fits_rpc<AddError: Serialize>(error: AddError, spec_method: &str) {
    let schema = get_starknet_spec_api_schema_for_method_errors(
        &[(SpecFile::StarknetWriteApi, &[spec_method])],
        &VERSION_0_4,
    );
    assert!(schema.is_valid(&serde_json::to_value(error).unwrap()));
}

const MESSAGE: &str = "message";

fn invoke_errors_and_starknet_error_codes() -> Vec<(AddInvokeError, StarknetErrorCode)> {
    vec![
        (
            AddInvokeError::InsufficientAccountBalance(InsufficientAccountBalance::Error(
                ErrorCode::default(),
            )),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InsufficientAccountBalance),
        ),
        (
            AddInvokeError::InsufficientMaxFee(InsufficientMaxFee::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InsufficientMaxFee),
        ),
        (
            AddInvokeError::InvalidTransactionNonce(InvalidTransactionNonce::Error(
                ErrorCode::default(),
            )),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce),
        ),
        (
            AddInvokeError::ValidationFailure(ValidationFailure::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::ValidateFailure),
        ),
        (
            AddInvokeError::NonAccount(NonAccount::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::EntryPointNotFoundInContract),
        ),
        (
            AddInvokeError::DuplicateTx(DuplicateTx::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::DuplicatedTransaction),
        ),
        (
            AddInvokeError::UnsupportedTxVersion(UnsupportedTxVersion::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionVersion),
        ),
        (
            AddInvokeError::UnexpectedError(UnexpectedError::Error(ErrorCodeWithData {
                data: MESSAGE.to_string(),
                ..Default::default()
            })),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::BlockNotFound),
        ),
        (
            AddInvokeError::UnexpectedError(UnexpectedError::Error(ErrorCodeWithData {
                data: MESSAGE.to_string(),
                ..Default::default()
            })),
            StarknetErrorCode::UnknownErrorCode("CODE".to_string()),
        ),
    ]
}

fn declare_errors_and_starknet_error_codes() -> Vec<(AddDeclareError, StarknetErrorCode)> {
    vec![
        (
            AddDeclareError::ClassAlreadyDeclared(
                ClassAlreadyDeclared::Error(ErrorCode::default()),
            ),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::ClassAlreadyDeclared),
        ),
        (
            AddDeclareError::CompilationFailed(CompilationFailed::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::CompilationFailed),
        ),
        (
            AddDeclareError::CompiledClassHashMismatch(CompiledClassHashMismatch::Error(
                ErrorCode::default(),
            )),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidCompiledClassHash),
        ),
        (
            AddDeclareError::InsufficientAccountBalance(InsufficientAccountBalance::Error(
                ErrorCode::default(),
            )),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InsufficientAccountBalance),
        ),
        (
            AddDeclareError::InsufficientMaxFee(InsufficientMaxFee::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InsufficientMaxFee),
        ),
        (
            AddDeclareError::InvalidTransactionNonce(InvalidTransactionNonce::Error(
                ErrorCode::default(),
            )),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce),
        ),
        (
            AddDeclareError::ValidationFailure(ValidationFailure::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::ValidateFailure),
        ),
        (
            AddDeclareError::NonAccount(NonAccount::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::EntryPointNotFoundInContract),
        ),
        (
            AddDeclareError::DuplicateTx(DuplicateTx::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::DuplicatedTransaction),
        ),
        (
            AddDeclareError::ContractClassSizeIsTooLarge(ContractClassSizeIsTooLarge::Error(
                ErrorCode::default(),
            )),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::ContractBytecodeSizeTooLarge),
        ),
        (
            AddDeclareError::ContractClassSizeIsTooLarge(ContractClassSizeIsTooLarge::Error(
                ErrorCode::default(),
            )),
            StarknetErrorCode::KnownErrorCode(
                KnownStarknetErrorCode::ContractClassObjectSizeTooLarge,
            ),
        ),
        (
            AddDeclareError::UnsupportedTxVersion(
                UnsupportedTxVersion::Error(ErrorCode::default()),
            ),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionVersion),
        ),
        (
            AddDeclareError::UnsupportedContractClassVersion(
                UnsupportedContractClassVersion::Error(ErrorCode::default()),
            ),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidContractClassVersion),
        ),
        (
            AddDeclareError::UnexpectedError(UnexpectedError::Error(ErrorCodeWithData {
                data: MESSAGE.to_string(),
                ..Default::default()
            })),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::BlockNotFound),
        ),
        (
            AddDeclareError::UnexpectedError(UnexpectedError::Error(ErrorCodeWithData {
                data: MESSAGE.to_string(),
                ..Default::default()
            })),
            StarknetErrorCode::UnknownErrorCode("CODE".to_string()),
        ),
    ]
}

fn deploy_account_errors_and_starknet_error_codes()
-> Vec<(AddDeployAccountError, StarknetErrorCode)> {
    vec![
        (
            AddDeployAccountError::InsufficientAccountBalance(InsufficientAccountBalance::Error(
                ErrorCode::default(),
            )),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InsufficientAccountBalance),
        ),
        (
            AddDeployAccountError::InsufficientMaxFee(InsufficientMaxFee::Error(
                ErrorCode::default(),
            )),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InsufficientMaxFee),
        ),
        (
            AddDeployAccountError::InvalidTransactionNonce(InvalidTransactionNonce::Error(
                ErrorCode::default(),
            )),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce),
        ),
        (
            AddDeployAccountError::ValidationFailure(
                ValidationFailure::Error(ErrorCode::default()),
            ),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::ValidateFailure),
        ),
        (
            AddDeployAccountError::NonAccount(NonAccount::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::EntryPointNotFoundInContract),
        ),
        (
            AddDeployAccountError::ClassHashNotFound(
                ClassHashNotFound::Error(ErrorCode::default()),
            ),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::UndeclaredClass),
        ),
        (
            AddDeployAccountError::DuplicateTx(DuplicateTx::Error(ErrorCode::default())),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::DuplicatedTransaction),
        ),
        (
            AddDeployAccountError::UnsupportedTxVersion(UnsupportedTxVersion::Error(
                ErrorCode::default(),
            )),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionVersion),
        ),
        (
            AddDeployAccountError::UnexpectedError(UnexpectedError::Error(ErrorCodeWithData {
                data: MESSAGE.to_string(),
                ..Default::default()
            })),
            StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::BlockNotFound),
        ),
        (
            AddDeployAccountError::UnexpectedError(UnexpectedError::Error(ErrorCodeWithData {
                data: MESSAGE.to_string(),
                ..Default::default()
            })),
            StarknetErrorCode::UnknownErrorCode("CODE".to_string()),
        ),
    ]
}

#[test]
fn add_invoke_error_fits_rpc() {
    for (error, _) in invoke_errors_and_starknet_error_codes() {
        test_error_fits_rpc(error, "starknet_addInvokeTransaction");
    }
}

#[test]
fn add_invoke_error_from_starknet_error() {
    for (error, starknet_error_code) in invoke_errors_and_starknet_error_codes() {
        assert_eq!(
            error,
            StarknetError { code: starknet_error_code, message: MESSAGE.to_string() }.into()
        );
    }
}

#[test]
fn add_declare_error_fits_rpc() {
    for (error, _) in declare_errors_and_starknet_error_codes() {
        test_error_fits_rpc(error, "starknet_addDeclareTransaction");
    }
}

#[test]
fn add_declare_error_from_starknet_error() {
    for (error, starknet_error_code) in declare_errors_and_starknet_error_codes() {
        assert_eq!(
            error,
            StarknetError { code: starknet_error_code, message: MESSAGE.to_string() }.into()
        );
    }
}

#[test]
fn add_deploy_account_error_fits_rpc() {
    for (error, _) in deploy_account_errors_and_starknet_error_codes() {
        test_error_fits_rpc(error, "starknet_addDeployAccountTransaction");
    }
}

#[test]
fn add_deploy_account_error_from_starknet_error() {
    for (error, starknet_error_code) in deploy_account_errors_and_starknet_error_codes() {
        assert_eq!(
            error,
            StarknetError { code: starknet_error_code, message: MESSAGE.to_string() }.into()
        );
    }
}
