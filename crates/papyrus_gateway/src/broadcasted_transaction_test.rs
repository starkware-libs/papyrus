use std::collections::HashMap;
use std::sync::Arc;

use futures::executor::block_on;
use starknet_api::deprecated_contract_class::{
    EntryPoint as DeprecatedEntryPoint, EntryPointType as DeprecatedEntryPointType, EventAbiEntry,
    FunctionAbiEntry, StructAbiEntry, StructMember, TypedParameter,
};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{EntryPoint, EntryPointType};
use starknet_api::transaction::{Calldata, TransactionSignature};
use starknet_writer_client::objects::transaction::{
    DeprecatedContractClass, DeprecatedContractClassAbiEntry,
};

use crate::broadcasted_transaction::{
    BroadcastedDeclareTransaction, BroadcastedDeclareV2Transaction, BroadcastedTransaction,
    ClientDeclareV1Transaction, ClientDeployAccountTransaction, ClientInvokeTransaction,
};
use crate::state::ContractClass;
use crate::test_utils::get_starknet_spec_api_schema;

fn validate_tx_fits_rpc(tx: BroadcastedTransaction) {
    let schema = block_on(get_starknet_spec_api_schema(&["BROADCASTED_TXN"]));
    assert!(schema.is_valid(&serde_json::to_value(tx).unwrap()));
}

#[test]
fn deploy_account_fits_rpc() {
    let tx = BroadcastedTransaction::DeployAccount(ClientDeployAccountTransaction {
        constructor_calldata: Calldata(Arc::new(vec![StarkFelt::default()])),
        signature: TransactionSignature(vec![StarkFelt::default()]),
        ..Default::default()
    });
    validate_tx_fits_rpc(tx);
}

#[test]
fn invoke_fits_rpc() {
    let tx = BroadcastedTransaction::Invoke(ClientInvokeTransaction {
        calldata: Calldata(Arc::new(vec![StarkFelt::default()])),
        signature: TransactionSignature(vec![StarkFelt::default()]),
        ..Default::default()
    });
    validate_tx_fits_rpc(tx);
}

#[test]
fn declare_v1_fits_rpc() {
    for has_abi in [true, false] {
        let abi = if has_abi {
            Some(vec![
                DeprecatedContractClassAbiEntry::Event(EventAbiEntry {
                    keys: vec![TypedParameter::default()],
                    data: vec![TypedParameter::default()],
                    ..Default::default()
                }),
                DeprecatedContractClassAbiEntry::Function(FunctionAbiEntry {
                    inputs: vec![TypedParameter::default()],
                    outputs: vec![TypedParameter::default()],
                    ..Default::default()
                }),
                DeprecatedContractClassAbiEntry::Constructor(FunctionAbiEntry {
                    inputs: vec![TypedParameter::default()],
                    outputs: vec![TypedParameter::default()],
                    ..Default::default()
                }),
                DeprecatedContractClassAbiEntry::L1Handler(FunctionAbiEntry {
                    inputs: vec![TypedParameter::default()],
                    outputs: vec![TypedParameter::default()],
                    ..Default::default()
                }),
                DeprecatedContractClassAbiEntry::Struct(StructAbiEntry {
                    members: vec![StructMember::default()],
                    // TODO(shahak) Change the default size of StructAbiEntry to be
                    // non-zero.
                    size: 1,
                    ..Default::default()
                }),
            ])
        } else {
            None
        };
        let tx = BroadcastedTransaction::Declare(BroadcastedDeclareTransaction::DeclareV1(
            ClientDeclareV1Transaction {
                signature: TransactionSignature(vec![StarkFelt::default()]),
                contract_class: DeprecatedContractClass {
                    abi,
                    entry_points_by_type: HashMap::from([
                        (
                            DeprecatedEntryPointType::Constructor,
                            vec![DeprecatedEntryPoint::default()],
                        ),
                        (DeprecatedEntryPointType::External, vec![DeprecatedEntryPoint::default()]),
                        (
                            DeprecatedEntryPointType::L1Handler,
                            vec![DeprecatedEntryPoint::default()],
                        ),
                    ]),
                    ..Default::default()
                },
                ..Default::default()
            },
        ));
        validate_tx_fits_rpc(tx);
    }
}

#[test]
fn declare_v2_fits_rpc() {
    let tx = BroadcastedTransaction::Declare(BroadcastedDeclareTransaction::DeclareV2(
        BroadcastedDeclareV2Transaction {
            signature: TransactionSignature(vec![StarkFelt::default()]),
            contract_class: ContractClass {
                sierra_program: vec![StarkFelt::default()],
                entry_points_by_type: HashMap::from([
                    (EntryPointType::Constructor, vec![EntryPoint::default()]),
                    (EntryPointType::External, vec![EntryPoint::default()]),
                    (EntryPointType::L1Handler, vec![EntryPoint::default()]),
                ]),
                ..Default::default()
            },
            ..Default::default()
        },
    ));
    validate_tx_fits_rpc(tx);
}
