use serde::Serialize;
use starknet_api::core::{ClassHash, ContractAddress, PatriciaKey};
use starknet_api::transaction::TransactionHash;
use starknet_client::writer::objects::response::{
    DeclareResponse,
    DeployAccountResponse,
    InvokeResponse,
    SuccessfulStarknetErrorCode,
};
use starknet_types_core::felt::Felt;
use test_utils::{auto_impl_get_test_instance, get_rng, GetTestInstance};

use super::{AddDeclareOkResult, AddDeployAccountOkResult, AddInvokeOkResult};
use crate::test_utils::{get_starknet_spec_api_schema_for_method_results, SpecFile};
use crate::version_config::VERSION_0_5 as VERSION;

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
        &[(SpecFile::WriteApi, &[spec_method])],
        &VERSION,
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
    test_ok_result_fits_rpc::<AddDeclareOkResult>("starknet_addDeclareTransaction");
}

#[test]
fn add_deploy_account_ok_result_fits_rpc() {
    test_ok_result_fits_rpc::<AddDeployAccountOkResult>("starknet_addDeployAccountTransaction");
}

#[test]
fn add_invoke_ok_result_from_response() {
    let transaction_hash = TransactionHash(Felt::from_hex_unchecked("0x12345"));
    let ok_result = AddInvokeOkResult::from(InvokeResponse {
        code: SuccessfulStarknetErrorCode::default(),
        transaction_hash,
    });
    let expected_ok_result = AddInvokeOkResult { transaction_hash };
    assert_eq!(expected_ok_result, ok_result);
}

#[test]
fn add_declare_ok_result_from_response() {
    let transaction_hash = TransactionHash(Felt::from_hex_unchecked("0x12345"));
    let class_hash = ClassHash(Felt::from_hex_unchecked("0xabcde"));
    let ok_result = AddDeclareOkResult::from(DeclareResponse {
        code: SuccessfulStarknetErrorCode::default(),
        transaction_hash,
        class_hash,
    });
    let expected_ok_result = AddDeclareOkResult { transaction_hash, class_hash };
    assert_eq!(expected_ok_result, ok_result);
}

#[test]
fn add_deploy_account_ok_result_from_response() {
    let transaction_hash = TransactionHash(Felt::from_hex_unchecked("0x12345"));
    let contract_address =
        ContractAddress(PatriciaKey::try_from(Felt::from_hex_unchecked("0xabcde")).unwrap());
    let ok_result = AddDeployAccountOkResult::from(DeployAccountResponse {
        code: SuccessfulStarknetErrorCode::default(),
        transaction_hash,
        address: contract_address,
    });
    let expected_ok_result = AddDeployAccountOkResult { transaction_hash, contract_address };
    assert_eq!(expected_ok_result, ok_result);
}
