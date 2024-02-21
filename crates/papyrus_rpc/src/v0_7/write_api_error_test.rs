use std::collections::BTreeMap;

use enum_iterator::all;
use jsonrpsee::types::ErrorObjectOwned;
use starknet_client::starknet_error::{KnownStarknetErrorCode, StarknetError, StarknetErrorCode};

use super::super::error::JsonRpcError;
use super::{
    starknet_error_to_declare_error,
    starknet_error_to_deploy_account_error,
    starknet_error_to_invoke_error,
};
use crate::test_utils::{get_starknet_spec_api_schema_for_method_errors, SpecFile};
use crate::version_config::VERSION_0_7 as Version;

const MESSAGE: &str = "message";
const UNKNOWN_CODE: &str = "code";

fn starknet_errors() -> impl Iterator<Item = StarknetError> {
    all::<KnownStarknetErrorCode>()
        .map(StarknetErrorCode::KnownErrorCode)
        .chain([StarknetErrorCode::UnknownErrorCode(UNKNOWN_CODE.to_owned())])
        .map(|error_code| StarknetError { code: error_code, message: MESSAGE.to_owned() })
}

fn test_error_from_conversion_fits_rpc<F: Fn(StarknetError) -> JsonRpcError<String>>(
    f: F,
    spec_method: &str,
) {
    let schema = get_starknet_spec_api_schema_for_method_errors(
        &[(SpecFile::WriteApi, &[spec_method])],
        &Version,
    );
    for starknet_error in starknet_errors() {
        // Converting into ErrorObjectOwned since it has serialization.
        let rpc_error: ErrorObjectOwned = f(starknet_error).into();
        let mut json_value = serde_json::to_value(rpc_error).unwrap();
        json_value.as_object_mut().unwrap().retain(|_, v| !v.is_null());
        assert!(schema.is_valid(&json_value));
    }
}

#[test]
fn starknet_error_to_invoke_error_result_fits_specs() {
    test_error_from_conversion_fits_rpc(
        starknet_error_to_invoke_error,
        "starknet_addInvokeTransaction",
    );
}

#[test]
fn starknet_error_to_declare_error_result_fits_specs() {
    test_error_from_conversion_fits_rpc(
        starknet_error_to_declare_error,
        "starknet_addDeclareTransaction",
    );
}

#[test]
fn starknet_error_to_deploy_account_error_result_fits_specs() {
    test_error_from_conversion_fits_rpc(
        starknet_error_to_deploy_account_error,
        "starknet_addDeployAccountTransaction",
    );
}

fn get_conversion_snapshot<F: Fn(StarknetError) -> JsonRpcError<String>>(
    f: F,
) -> serde_json::Value {
    // Using BTreeMap to keep the keys sorted.
    let mut rpc_error_code_to_errors = BTreeMap::new();
    for starknet_error in starknet_errors() {
        let rpc_error: ErrorObjectOwned = f(starknet_error.clone()).into();
        rpc_error_code_to_errors.insert(rpc_error.code(), (rpc_error, starknet_error));
    }
    serde_json::to_value(rpc_error_code_to_errors).unwrap()
}

#[test]
fn starknet_error_to_invoke_error_snapshot() {
    insta::assert_json_snapshot!(get_conversion_snapshot(starknet_error_to_invoke_error));
}

#[test]
fn starknet_error_to_declare_error_snapshot() {
    insta::assert_json_snapshot!(get_conversion_snapshot(starknet_error_to_declare_error));
}

#[test]
fn starknet_error_to_deploy_account_error_snapshot() {
    insta::assert_json_snapshot!(get_conversion_snapshot(starknet_error_to_deploy_account_error));
}
