use enum_iterator::all;
use jsonrpsee::types::ErrorObjectOwned;
use starknet_client::starknet_error::{KnownStarknetErrorCode, StarknetError, StarknetErrorCode};

use super::super::error::JsonRpcError;
use super::{
    starknet_error_to_declare_error, starknet_error_to_deploy_account_error,
    starknet_error_to_invoke_error,
};
use crate::test_utils::{get_starknet_spec_api_schema_for_method_errors, SpecFile};
use crate::version_config::VERSION_0_4;

const MESSAGE: &str = "message";
const UNKNOWN_CODE: &str = "code";

fn test_error_from_conversion_fits_rpc<F: Fn(StarknetError) -> JsonRpcError>(
    f: F,
    spec_method: &str,
) {
    let schema = get_starknet_spec_api_schema_for_method_errors(
        &[(SpecFile::StarknetWriteApi, &[spec_method])],
        &VERSION_0_4,
    );
    for starknet_error_code in all::<KnownStarknetErrorCode>()
        .map(|known_error_code| StarknetErrorCode::KnownErrorCode(known_error_code))
        .chain([StarknetErrorCode::UnknownErrorCode(UNKNOWN_CODE.to_owned())])
    {
        let starknet_error =
            StarknetError { code: starknet_error_code, message: MESSAGE.to_owned() };
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
