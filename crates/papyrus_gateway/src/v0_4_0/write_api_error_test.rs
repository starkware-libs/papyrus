use serde::Serialize;

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

#[test]
fn add_invoke_error_fits_rpc() {
    for error in [
        AddInvokeError::InsufficientAccountBalance(InsufficientAccountBalance::Error(
            ErrorCode::default(),
        )),
        AddInvokeError::InsufficientMaxFee(InsufficientMaxFee::Error(ErrorCode::default())),
        AddInvokeError::InvalidTransactionNonce(InvalidTransactionNonce::Error(
            ErrorCode::default(),
        )),
        AddInvokeError::ValidationFailure(ValidationFailure::Error(ErrorCode::default())),
        AddInvokeError::NonAccount(NonAccount::Error(ErrorCode::default())),
        AddInvokeError::DuplicateTx(DuplicateTx::Error(ErrorCode::default())),
        AddInvokeError::UnsupportedTxVersion(UnsupportedTxVersion::Error(ErrorCode::default())),
        AddInvokeError::UnexpectedError(UnexpectedError::Error(ErrorCodeWithData {
            data: "data".to_string(),
            ..Default::default()
        })),
    ] {
        test_error_fits_rpc(error, "starknet_addInvokeTransaction");
    }
}

#[test]
fn add_declare_error_fits_rpc() {
    for error in [
        AddDeclareError::ClassAlreadyDeclared(ClassAlreadyDeclared::Error(ErrorCode::default())),
        AddDeclareError::CompilationFailed(CompilationFailed::Error(ErrorCode::default())),
        AddDeclareError::CompiledClassHashMismatch(CompiledClassHashMismatch::Error(
            ErrorCode::default(),
        )),
        AddDeclareError::InsufficientAccountBalance(InsufficientAccountBalance::Error(
            ErrorCode::default(),
        )),
        AddDeclareError::InsufficientMaxFee(InsufficientMaxFee::Error(ErrorCode::default())),
        AddDeclareError::InvalidTransactionNonce(InvalidTransactionNonce::Error(
            ErrorCode::default(),
        )),
        AddDeclareError::ValidationFailure(ValidationFailure::Error(ErrorCode::default())),
        AddDeclareError::NonAccount(NonAccount::Error(ErrorCode::default())),
        AddDeclareError::DuplicateTx(DuplicateTx::Error(ErrorCode::default())),
        AddDeclareError::ContractClassSizeIsTooLarge(ContractClassSizeIsTooLarge::Error(
            ErrorCode::default(),
        )),
        AddDeclareError::UnsupportedTxVersion(UnsupportedTxVersion::Error(ErrorCode::default())),
        AddDeclareError::UnsupportedContractClassVersion(UnsupportedContractClassVersion::Error(
            ErrorCode::default(),
        )),
        AddDeclareError::UnexpectedError(UnexpectedError::Error(ErrorCodeWithData {
            data: "data".to_string(),
            ..Default::default()
        })),
    ] {
        test_error_fits_rpc(error, "starknet_addDeclareTransaction");
    }
}

#[test]
fn add_deploy_account_error_fits_rpc() {
    for error in [
        AddDeployAccountError::InsufficientAccountBalance(InsufficientAccountBalance::Error(
            ErrorCode::default(),
        )),
        AddDeployAccountError::InsufficientMaxFee(InsufficientMaxFee::Error(ErrorCode::default())),
        AddDeployAccountError::InvalidTransactionNonce(InvalidTransactionNonce::Error(
            ErrorCode::default(),
        )),
        AddDeployAccountError::ValidationFailure(ValidationFailure::Error(ErrorCode::default())),
        AddDeployAccountError::NonAccount(NonAccount::Error(ErrorCode::default())),
        AddDeployAccountError::ClassHashNotFound(ClassHashNotFound::Error(ErrorCode::default())),
        AddDeployAccountError::DuplicateTx(DuplicateTx::Error(ErrorCode::default())),
        AddDeployAccountError::UnsupportedTxVersion(UnsupportedTxVersion::Error(
            ErrorCode::default(),
        )),
        AddDeployAccountError::UnexpectedError(UnexpectedError::Error(ErrorCodeWithData {
            data: "data".to_string(),
            ..Default::default()
        })),
    ] {
        test_error_fits_rpc(error, "starknet_addDeployAccountTransaction");
    }
}
