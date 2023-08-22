use std::collections::HashMap;

use jsonschema::JSONSchema;
use lazy_static::lazy_static;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::{
    ContractClassAbiEntry as DeprecatedContractClassAbiEntry,
    EntryPoint as DeprecatedEntryPoint,
    EntryPointType as DeprecatedEntryPointType,
    EventAbiEntry,
    FunctionAbiEntry,
    StructAbiEntry,
};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{EntryPoint, EntryPointType};
use starknet_api::transaction::{
    Calldata,
    ContractAddressSalt,
    Fee,
    TransactionSignature,
    TransactionVersion,
};
use starknet_client::writer::objects::transaction::DeprecatedContractClass;
use test_utils::{auto_impl_get_test_instance, get_rng, GetTestInstance};

use super::super::state::ContractClass;
use super::{
    BroadcastedDeclareTransaction,
    BroadcastedDeclareV1Transaction,
    BroadcastedDeclareV2Transaction,
    BroadcastedDeployAccountTransaction,
    BroadcastedInvokeTransaction,
    BroadcastedTransaction,
};
use crate::test_utils::{get_starknet_spec_api_schema_for_components, SpecFile};
use crate::version_config::VERSION_0_3;

auto_impl_get_test_instance! {
    pub struct BroadcastedDeployAccountTransaction {
        pub contract_address_salt: ContractAddressSalt,
        pub class_hash: ClassHash,
        pub constructor_calldata: Calldata,
        pub nonce: Nonce,
        pub max_fee: Fee,
        pub signature: TransactionSignature,
        pub version: TransactionVersion,
    }
    pub struct BroadcastedInvokeTransaction {
        pub calldata: Calldata,
        pub sender_address: ContractAddress,
        pub nonce: Nonce,
        pub max_fee: Fee,
        pub signature: TransactionSignature,
        pub version: TransactionVersion,
    }
    pub struct BroadcastedDeclareV1Transaction {
        pub contract_class: DeprecatedContractClass,
        pub sender_address: ContractAddress,
        pub nonce: Nonce,
        pub max_fee: Fee,
        pub version: TransactionVersion,
        pub signature: TransactionSignature,
    }
    pub struct BroadcastedDeclareV2Transaction {
        pub contract_class: ContractClass,
        pub compiled_class_hash: CompiledClassHash,
        pub sender_address: ContractAddress,
        pub nonce: Nonce,
        pub max_fee: Fee,
        pub version: TransactionVersion,
        pub signature: TransactionSignature,
    }
    pub struct ContractClass {
        pub sierra_program: Vec<StarkFelt>,
        pub contract_class_version: String,
        pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
        pub abi: String,
    }
}

fn validate_tx_fits_rpc(tx: BroadcastedTransaction) {
    lazy_static! {
        static ref SCHEMA: JSONSchema = get_starknet_spec_api_schema_for_components(
            &[(SpecFile::StarknetApiOpenrpc, &["BROADCASTED_TXN"])],
            &VERSION_0_3
        );
    }
    assert!(SCHEMA.is_valid(&serde_json::to_value(tx).unwrap()));
}

#[test]
fn deploy_account_fits_rpc() {
    let tx = BroadcastedTransaction::DeployAccount(
        BroadcastedDeployAccountTransaction::get_test_instance(&mut get_rng()),
    );
    validate_tx_fits_rpc(tx);
}

#[test]
fn invoke_fits_rpc() {
    let tx = BroadcastedTransaction::Invoke(BroadcastedInvokeTransaction::get_test_instance(
        &mut get_rng(),
    ));
    validate_tx_fits_rpc(tx);
}

// TODO(shahak): Fix entry_points_by_type and re-enable this test.
#[ignore]
#[test]
fn declare_v1_fits_rpc() {
    let mut rng = get_rng();
    for has_abi in [true, false] {
        let abi = if has_abi {
            Some(vec![
                DeprecatedContractClassAbiEntry::Event(EventAbiEntry::get_test_instance(&mut rng)),
                DeprecatedContractClassAbiEntry::Function(FunctionAbiEntry::get_test_instance(
                    &mut rng,
                )),
                DeprecatedContractClassAbiEntry::Constructor(FunctionAbiEntry::get_test_instance(
                    &mut rng,
                )),
                DeprecatedContractClassAbiEntry::L1Handler(FunctionAbiEntry::get_test_instance(
                    &mut rng,
                )),
                DeprecatedContractClassAbiEntry::Struct(StructAbiEntry::get_test_instance(
                    &mut rng,
                )),
            ])
        } else {
            None
        };
        let tx = BroadcastedTransaction::Declare(BroadcastedDeclareTransaction::DeclareV1(
            BroadcastedDeclareV1Transaction {
                contract_class: DeprecatedContractClass {
                    abi,
                    entry_points_by_type: HashMap::from([
                        (
                            DeprecatedEntryPointType::Constructor,
                            vec![DeprecatedEntryPoint::get_test_instance(&mut rng)],
                        ),
                        (
                            DeprecatedEntryPointType::External,
                            vec![DeprecatedEntryPoint::get_test_instance(&mut rng)],
                        ),
                        (
                            DeprecatedEntryPointType::L1Handler,
                            vec![DeprecatedEntryPoint::get_test_instance(&mut rng)],
                        ),
                    ]),
                    ..GetTestInstance::get_test_instance(&mut rng)
                },
                ..GetTestInstance::get_test_instance(&mut rng)
            },
        ));
        validate_tx_fits_rpc(tx);
    }
}

#[test]
fn declare_v2_fits_rpc() {
    let mut rng = get_rng();
    let tx = BroadcastedTransaction::Declare(BroadcastedDeclareTransaction::DeclareV2(
        BroadcastedDeclareV2Transaction {
            contract_class: ContractClass {
                entry_points_by_type: HashMap::from([
                    (EntryPointType::Constructor, Vec::<EntryPoint>::get_test_instance(&mut rng)),
                    (EntryPointType::External, Vec::<EntryPoint>::get_test_instance(&mut rng)),
                    (EntryPointType::L1Handler, Vec::<EntryPoint>::get_test_instance(&mut rng)),
                ]),
                ..GetTestInstance::get_test_instance(&mut rng)
            },
            ..GetTestInstance::get_test_instance(&mut rng)
        },
    ));
    validate_tx_fits_rpc(tx);
}
