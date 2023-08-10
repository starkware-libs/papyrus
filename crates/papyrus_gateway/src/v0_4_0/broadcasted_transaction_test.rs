use std::collections::HashMap;

use jsonschema::JSONSchema;
use lazy_static::lazy_static;
use starknet_api::core::{CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::{
    ContractClassAbiEntry as DeprecatedContractClassAbiEntry,
    EntryPoint as DeprecatedEntryPoint,
    EntryPointType as DeprecatedEntryPointType,
    EventAbiEntry,
    FunctionAbiEntry,
    StructAbiEntry,
};
use starknet_api::hash::StarkFelt;
use starknet_api::state::EntryPoint;
use starknet_api::transaction::{Fee, TransactionSignature};
use starknet_client::writer::objects::transaction::DeprecatedContractClass;
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, get_rng, GetTestInstance};

use super::broadcasted_transaction::{
    BroadcastedDeclareTransaction,
    BroadcastedDeclareV1Transaction,
    BroadcastedDeclareV2Transaction,
    DeclareType,
};
use super::state::{ContractClass, EntryPointByType};
use crate::test_utils::{get_starknet_spec_api_schema_for_components, SpecFile};
use crate::version_config::VERSION_0_4;

fn validate_tx_fits_rpc(tx: BroadcastedDeclareTransaction) {
    lazy_static! {
        static ref SCHEMA: JSONSchema = get_starknet_spec_api_schema_for_components(
            &[(SpecFile::StarknetApiOpenrpc, &["DECLARE_TXN"])],
            &VERSION_0_4
        );
    }
    assert!(SCHEMA.is_valid(&serde_json::to_value(tx).unwrap()));
}

auto_impl_get_test_instance! {
    pub struct BroadcastedDeclareV1Transaction {
        pub r#type: DeclareType,
        pub contract_class: DeprecatedContractClass,
        pub sender_address: ContractAddress,
        pub nonce: Nonce,
        pub max_fee: Fee,
        pub signature: TransactionSignature,
    }
    pub struct BroadcastedDeclareV2Transaction {
        pub r#type: DeclareType,
        pub contract_class: ContractClass,
        pub compiled_class_hash: CompiledClassHash,
        pub sender_address: ContractAddress,
        pub nonce: Nonce,
        pub max_fee: Fee,
        pub signature: TransactionSignature,
    }
    // TODO(yair): Move out the test instances of ContractClass and EntryPointByType.
    pub struct ContractClass {
        pub sierra_program: Vec<StarkFelt>,
        pub contract_class_version: String,
        pub entry_points_by_type: EntryPointByType,
        pub abi: String,
    }
    pub struct EntryPointByType {
        pub contructor: Vec<EntryPoint>,
        pub external: Vec<EntryPoint>,
        pub l1handler: Vec<EntryPoint>,
    }
    pub enum DeclareType {
        Declare = 0,
    }
}

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
        let inner_tx = BroadcastedDeclareV1Transaction {
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
        };
        let tx = BroadcastedDeclareTransaction::V1(inner_tx.clone());
        let schema: JSONSchema = get_starknet_spec_api_schema_for_components(
            &[(SpecFile::StarknetApiOpenrpc, &["DEPRECATED_CONTRACT_CLASS"])],
            &VERSION_0_4,
        );
        if has_abi {
            let schema: JSONSchema = get_starknet_spec_api_schema_for_components(
                &[(SpecFile::StarknetApiOpenrpc, &["CONTRACT_ABI"])],
                &VERSION_0_4,
            );
            assert!(
                schema
                    .is_valid(&serde_json::to_value(inner_tx.clone().contract_class.abi).unwrap())
            );
        }
        assert!(schema.is_valid(&serde_json::to_value(inner_tx.clone().contract_class).unwrap()));
        validate_tx_fits_rpc(tx);
    }
}

#[test]
fn declare_v2_fits_rpc() {
    let mut rng = get_rng();
    let tx = BroadcastedDeclareTransaction::V2(BroadcastedDeclareV2Transaction::get_test_instance(
        &mut rng,
    ));
    validate_tx_fits_rpc(tx);
}
