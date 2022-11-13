use std::fmt::Debug;

use starknet_api::serde_utils::bytes_from_hex_str;
use starknet_api::{
    shash, BlockHash, BlockNumber, BlockStatus, CallData, ClassHash, ContractAddress,
    ContractAddressSalt, ContractClassAbiEntry, ContractNonce, DeclareTransaction,
    DeclaredContract, DeployAccountTransaction, DeployTransaction, DeployedContract,
    EntryPointSelector, EthAddress, EventAbiEntry, EventContent, EventData,
    EventIndexInTransactionOutput, EventKey, Fee, FunctionAbiEntry, FunctionAbiEntryType,
    FunctionAbiEntryWithType, InvokeTransaction, L1HandlerTransaction, L1ToL2Payload,
    L2ToL1Payload, MessageToL1, MessageToL2, Nonce, StarkHash, StateDiff, StorageDiff,
    StorageEntry, StorageKey, StructAbiEntry, StructMember, Transaction, TransactionHash,
    TransactionOffsetInBlock, TransactionSignature, TransactionVersion, TypedParameter,
};
use web3::types::H160;

use crate::body::events::{
    ThinDeclareTransactionOutput, ThinDeployAccountTransactionOutput, ThinDeployTransactionOutput,
    ThinInvokeTransactionOutput, ThinL1HandlerTransactionOutput,
};
use crate::state::{IndexedDeclaredContract, IndexedDeployedContract};
use crate::test_utils::{get_test_contract_class, get_test_header, get_test_program};
use crate::{
    EventIndex, MarkerKind, OmmerEventKey, OmmerTransactionKey, StorageSerde, ThinStateDiff,
    ThinTransactionOutput, TransactionIndex,
};

fn serde_item<I, F>(f: F) -> Result<(), anyhow::Error>
where
    F: Fn() -> Result<I, anyhow::Error>,
    I: StorageSerde + Eq + Debug,
{
    let item = f()?;
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = I::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn serde() -> Result<(), anyhow::Error> {
    // Test enums.
    serde_item(|| Ok(BlockStatus::AcceptedOnL1))?;
    serde_item(|| Ok(MarkerKind::Header))?;
    serde_item(|| Ok(Transaction::Declare(get_test_declare_transaction()?)))?;
    serde_item(|| Ok(ThinTransactionOutput::Declare(get_test_thin_declare_transaction_output()?)))?;
    serde_item(|| Ok(ContractClassAbiEntry::Event(get_test_event_abi_entry()?)))?;

    // Test tuple structs.
    serde_item(|| {
        Ok(EventIndex(
            TransactionIndex(BlockNumber::new(0), TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(0),
        ))
    })?;
    serde_item(|| {
        Ok(OmmerEventKey(
            OmmerTransactionKey(BlockHash::new(shash!("0x1")), TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(0),
        ))
    })?;

    // Test structs from test_utils.
    serde_item(|| Ok(get_test_header()))?;
    serde_item(|| Ok(get_test_contract_class()))?;
    serde_item(|| Ok(get_test_program()))?;

    // Test transactions.
    serde_item(get_test_declare_transaction)?;
    serde_item(get_test_deploy_account_transaction)?;
    serde_item(get_test_deploy_transaction)?;
    serde_item(get_test_invoke_transaction)?;
    serde_item(get_test_l1_handler_transaction)?;

    // Test transaction outputs.
    serde_item(get_test_thin_deploy_account_transaction_output)?;
    serde_item(get_test_thin_deploy_transaction_output)?;
    serde_item(get_test_thin_invoke_transaction_output)?;
    serde_item(get_test_thin_l1_handler_transaction_output)?;

    // Test abi entries.
    serde_item(get_test_event_abi_entry)?;
    serde_item(get_test_function_abi_entry)?;
    serde_item(get_test_function_abi_entry_with_type)?;
    serde_item(get_test_struct_abi_entry)?;

    // Test the rest of the structs that weren't tested.
    serde_item(get_test_event_content)?;
    serde_item(get_test_message_to_l2)?;
    serde_item(get_test_thin_state_diff)?;
    serde_item(get_test_indexed_declared_contract)?;
    serde_item(get_test_indexed_deployed_contract)?;
    Ok(())
}

fn get_test_declare_transaction() -> Result<DeclareTransaction, anyhow::Error> {
    Ok(DeclareTransaction {
        transaction_hash: TransactionHash(shash!("0x1")),
        max_fee: Fee(0),
        version: TransactionVersion(shash!("0x1")),
        signature: TransactionSignature(vec![shash!("0x1")]),
        nonce: Nonce::new(shash!("0x1")),
        class_hash: ClassHash::new(shash!("0x1")),
        sender_address: ContractAddress::try_from(shash!("0x1"))?,
    })
}

fn get_test_deploy_account_transaction() -> Result<DeployAccountTransaction, anyhow::Error> {
    Ok(DeployAccountTransaction {
        transaction_hash: TransactionHash(shash!("0x1")),
        max_fee: Fee(0),
        version: TransactionVersion(shash!("0x1")),
        signature: TransactionSignature(vec![shash!("0x1")]),
        nonce: Nonce::new(shash!("0x1")),
        class_hash: ClassHash::new(shash!("0x1")),
        contract_address: ContractAddress::try_from(shash!("0x1"))?,
        contract_address_salt: ContractAddressSalt(shash!("0x1")),
        constructor_calldata: CallData(vec![shash!("0x1")]),
    })
}

fn get_test_deploy_transaction() -> Result<DeployTransaction, anyhow::Error> {
    Ok(DeployTransaction {
        transaction_hash: TransactionHash(shash!("0x1")),
        version: TransactionVersion(shash!("0x1")),
        class_hash: ClassHash::new(shash!("0x1")),
        contract_address: ContractAddress::try_from(shash!("0x1"))?,
        contract_address_salt: ContractAddressSalt(shash!("0x1")),
        constructor_calldata: CallData(vec![shash!("0x1")]),
    })
}

fn get_test_event_abi_entry() -> Result<EventAbiEntry, anyhow::Error> {
    Ok(EventAbiEntry {
        name: "a".to_string(),
        keys: vec![TypedParameter { name: "a".to_string(), r#type: "a".to_string() }],
        data: vec![TypedParameter { name: "a".to_string(), r#type: "a".to_string() }],
    })
}

fn get_test_event_content() -> Result<EventContent, anyhow::Error> {
    Ok(EventContent { keys: vec![EventKey(shash!("0x1"))], data: EventData(vec![shash!("0x0")]) })
}

fn get_test_function_abi_entry() -> Result<FunctionAbiEntry, anyhow::Error> {
    Ok(FunctionAbiEntry {
        name: "a".to_string(),
        inputs: vec![TypedParameter { name: "a".to_string(), r#type: "a".to_string() }],
        outputs: vec![TypedParameter { name: "a".to_string(), r#type: "a".to_string() }],
    })
}

fn get_test_function_abi_entry_with_type() -> Result<FunctionAbiEntryWithType, anyhow::Error> {
    Ok(FunctionAbiEntryWithType {
        r#type: FunctionAbiEntryType::Constructor,
        entry: get_test_function_abi_entry()?,
    })
}

fn get_test_indexed_declared_contract() -> Result<IndexedDeclaredContract, anyhow::Error> {
    Ok(IndexedDeclaredContract {
        block_number: BlockNumber::new(0),
        contract_class: get_test_contract_class(),
    })
}

fn get_test_indexed_deployed_contract() -> Result<IndexedDeployedContract, anyhow::Error> {
    Ok(IndexedDeployedContract {
        block_number: BlockNumber::new(0),
        class_hash: ClassHash::new(shash!("0x1")),
    })
}

fn get_test_invoke_transaction() -> Result<InvokeTransaction, anyhow::Error> {
    Ok(InvokeTransaction {
        transaction_hash: TransactionHash(shash!("0x1")),
        max_fee: Fee(0),
        version: TransactionVersion(shash!("0x1")),
        signature: TransactionSignature(vec![shash!("0x1")]),
        nonce: Nonce::new(shash!("0x1")),
        contract_address: ContractAddress::try_from(shash!("0x1"))?,
        entry_point_selector: Some(EntryPointSelector(shash!("0x1"))),
        calldata: CallData(vec![shash!("0x1")]),
    })
}

fn get_test_l1_handler_transaction() -> Result<L1HandlerTransaction, anyhow::Error> {
    Ok(L1HandlerTransaction {
        transaction_hash: TransactionHash(shash!("0x1")),
        version: TransactionVersion(shash!("0x1")),
        nonce: Nonce::new(shash!("0x1")),
        contract_address: ContractAddress::try_from(shash!("0x1"))?,
        entry_point_selector: EntryPointSelector(shash!("0x1")),
        calldata: CallData(vec![shash!("0x1")]),
    })
}

fn get_test_message_to_l2() -> Result<MessageToL2, anyhow::Error> {
    Ok(MessageToL2 {
        from_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
        payload: L1ToL2Payload(vec![shash!("0x1")]),
    })
}

fn get_test_struct_abi_entry() -> Result<StructAbiEntry, anyhow::Error> {
    Ok(StructAbiEntry {
        name: "a".to_string(),
        size: 1,
        members: vec![StructMember {
            param: TypedParameter { name: "a".to_string(), r#type: "a".to_string() },
            offset: 1,
        }],
    })
}

fn get_test_thin_declare_transaction_output() -> Result<ThinDeclareTransactionOutput, anyhow::Error>
{
    Ok(ThinDeclareTransactionOutput {
        actual_fee: Fee(0),
        messages_sent: vec![MessageToL1 {
            to_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
            payload: L2ToL1Payload(vec![shash!("0x1")]),
        }],
        events_contract_addresses: vec![ContractAddress::try_from(shash!("0x1"))?],
    })
}

fn get_test_thin_deploy_account_transaction_output()
-> Result<ThinDeployAccountTransactionOutput, anyhow::Error> {
    Ok(ThinDeployAccountTransactionOutput {
        actual_fee: Fee(0),
        messages_sent: vec![MessageToL1 {
            to_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
            payload: L2ToL1Payload(vec![shash!("0x1")]),
        }],
        events_contract_addresses: vec![ContractAddress::try_from(shash!("0x1"))?],
    })
}

fn get_test_thin_deploy_transaction_output() -> Result<ThinDeployTransactionOutput, anyhow::Error> {
    Ok(ThinDeployTransactionOutput {
        actual_fee: Fee(0),
        messages_sent: vec![MessageToL1 {
            to_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
            payload: L2ToL1Payload(vec![shash!("0x1")]),
        }],
        events_contract_addresses: vec![ContractAddress::try_from(shash!("0x1"))?],
    })
}

fn get_test_thin_invoke_transaction_output() -> Result<ThinInvokeTransactionOutput, anyhow::Error> {
    Ok(ThinInvokeTransactionOutput {
        actual_fee: Fee(0),
        messages_sent: vec![MessageToL1 {
            to_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
            payload: L2ToL1Payload(vec![shash!("0x1")]),
        }],
        events_contract_addresses: vec![ContractAddress::try_from(shash!("0x1"))?],
    })
}

fn get_test_thin_l1_handler_transaction_output()
-> Result<ThinL1HandlerTransactionOutput, anyhow::Error> {
    Ok(ThinL1HandlerTransactionOutput {
        actual_fee: Fee(0),
        messages_sent: vec![MessageToL1 {
            to_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
            payload: L2ToL1Payload(vec![shash!("0x1")]),
        }],
        events_contract_addresses: vec![ContractAddress::try_from(shash!("0x1"))?],
    })
}

fn get_test_thin_state_diff() -> Result<ThinStateDiff, anyhow::Error> {
    let address = ContractAddress::try_from(shash!("0x1"))?;
    let state_diff = StateDiff::new(
        vec![DeployedContract { address, class_hash: ClassHash::new(shash!("0x1")) }],
        vec![StorageDiff {
            address,
            storage_entries: vec![StorageEntry {
                key: StorageKey::try_from(shash!("0x1"))?,
                value: shash!("0x1"),
            }],
        }],
        vec![DeclaredContract {
            class_hash: ClassHash::new(shash!("0x1")),
            contract_class: get_test_contract_class(),
        }],
        vec![ContractNonce { contract_address: address, nonce: Nonce::new(shash!("0x1")) }],
    )?;
    Ok(ThinStateDiff::from(state_diff))
}
