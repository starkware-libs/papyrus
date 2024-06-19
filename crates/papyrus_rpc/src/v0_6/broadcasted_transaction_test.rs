use std::collections::HashMap;

use jsonschema::JSONSchema;
use lazy_static::lazy_static;
use starknet_api::core::{CompiledClassHash, ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::deprecated_contract_class::{
    ContractClassAbiEntry as DeprecatedContractClassAbiEntry,
    EntryPoint as DeprecatedEntryPoint,
    EntryPointType as DeprecatedEntryPointType,
    EventAbiEntry,
    FunctionAbiEntry,
    StructAbiEntry,
};
use starknet_api::state::EntryPoint;
use starknet_api::transaction::{
    AccountDeploymentData,
    Fee,
    PaymasterData,
    ResourceBounds,
    Tip,
    TransactionSignature,
};
use starknet_client::writer::objects::transaction::DeprecatedContractClass;
use starknet_types_core::felt::Felt;
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, get_rng, GetTestInstance};

use super::super::state::{ContractClass, EntryPointByType};
use super::{
    BroadcastedDeclareTransaction,
    BroadcastedDeclareV1Transaction,
    BroadcastedDeclareV2Transaction,
    BroadcastedDeclareV3Transaction,
    DeclareType,
    ResourceBoundsMapping,
};
use crate::test_utils::{get_starknet_spec_api_schema_for_components, SpecFile};
use crate::version_config::VERSION_0_6 as Version;

fn validate_tx_fits_rpc(tx: BroadcastedDeclareTransaction) {
    lazy_static! {
        static ref SCHEMA: JSONSchema = get_starknet_spec_api_schema_for_components(
            &[(SpecFile::StarknetApiOpenrpc, &["BROADCASTED_DECLARE_TXN"])],
            &Version
        );
    }
    assert!(SCHEMA.is_valid(&serde_json::to_value(tx).unwrap()));
}

auto_impl_get_test_instance! {
    pub enum BroadcastedDeclareTransaction {
        V1(BroadcastedDeclareV1Transaction) = 0,
        V2(BroadcastedDeclareV2Transaction) = 1,
        V3(BroadcastedDeclareV3Transaction) = 2,
    }
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
    pub struct BroadcastedDeclareV3Transaction {
        pub r#type: DeclareType,
        pub sender_address: ContractAddress,
        pub compiled_class_hash: CompiledClassHash,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub contract_class: ContractClass,
        pub resource_bounds: ResourceBoundsMapping,
        pub tip: Tip,
        pub paymaster_data: PaymasterData,
        pub account_deployment_data: AccountDeploymentData,
        pub nonce_data_availability_mode: DataAvailabilityMode,
        pub fee_data_availability_mode: DataAvailabilityMode,
    }
    // TODO(yair): Move out the test instances of ContractClass and EntryPointByType.
    pub struct ContractClass {
        pub sierra_program: Vec<Felt>,
        pub contract_class_version: String,
        pub entry_points_by_type: EntryPointByType,
        pub abi: String,
    }

    pub struct EntryPointByType {
        pub constructor: Vec<EntryPoint>,
        pub external: Vec<EntryPoint>,
        pub l1handler: Vec<EntryPoint>,
    }
    pub enum DeclareType {
        Declare = 0,
    }

    pub struct ResourceBoundsMapping {
        pub l1_gas: ResourceBounds,
        pub l2_gas: ResourceBounds,
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
        let tx = BroadcastedDeclareTransaction::V1(BroadcastedDeclareV1Transaction {
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
        });
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

#[test]
fn declare_v3_fits_rpc() {
    let mut rng = get_rng();
    let tx = BroadcastedDeclareTransaction::V3(BroadcastedDeclareV3Transaction::get_test_instance(
        &mut rng,
    ));

    validate_tx_fits_rpc(tx);
}
