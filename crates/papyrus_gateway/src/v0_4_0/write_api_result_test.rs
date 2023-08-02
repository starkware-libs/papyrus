use serde::Serialize;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::transaction::TransactionHash;
use test_utils::{auto_impl_get_test_instance, get_rng, GetTestInstance};

use super::{AddDeclareOkResult, AddDeployAccountOkResult, AddInvokeOkResult};
use crate::test_utils::{get_starknet_spec_api_schema_for_method_results, SpecFile};
use crate::version_config::VERSION_0_4;

auto_impl_get_test_instance! {
    pub struct AddInvokeOkResult {
        pub transaction_hash: TransactionHash,
    }
    pub struct AddDeclareOkResult {
        pub transaction_hash: TransactionHash,
        pub class_hash: ClassHash,
    }
    pub struct AddDeployAccountOkResult {
        pub transaction_hash: TransactionHash,
        pub contract_address: ContractAddress,
    }
}

fn test_ok_result_fits_rpc<AddOkResult: GetTestInstance + Serialize>(spec_method: &str) {
    let schema = get_starknet_spec_api_schema_for_method_results(
        &[(SpecFile::StarknetWriteApi, &[spec_method])],
        &VERSION_0_4,
    );
    let result = AddOkResult::get_test_instance(&mut get_rng());
    assert!(schema.is_valid(&serde_json::to_value(result).unwrap()));
}

#[test]
fn add_invoke_ok_result_fits_rpc() {
    test_ok_result_fits_rpc::<AddInvokeOkResult>("starknet_addInvokeTransaction");
}

#[test]
fn add_declare_ok_result_fits_rpc() {
    test_ok_result_fits_rpc::<AddDeclareOkResult>("starknet_addInvokeTransaction");
}

#[test]
fn add_deploy_account_ok_result_fits_rpc() {
    test_ok_result_fits_rpc::<AddDeployAccountOkResult>("starknet_addInvokeTransaction");
}
