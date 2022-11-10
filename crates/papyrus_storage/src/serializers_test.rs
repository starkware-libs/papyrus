use starknet_api::serde_utils::bytes_from_hex_str;
use starknet_api::{
    shash, BlockHash, BlockHeader, BlockNumber, BlockStatus, CallData, ClassHash, ContractAddress,
    ContractAddressSalt, ContractClass, ContractClassAbiEntry, ContractNonce, DeclareTransaction,
    DeclaredContract, DeployAccountTransaction, DeployTransaction, DeployedContract,
    EntryPointSelector, EthAddress, EventAbiEntry, EventContent, EventData,
    EventIndexInTransactionOutput, EventKey, Fee, FunctionAbiEntry, InvokeTransaction,
    L1HandlerTransaction, L1ToL2Payload, L2ToL1Payload, MessageToL1, MessageToL2, Nonce, Program,
    StarkHash, StateDiff, StorageDiff, StorageEntry, StorageKey, StructAbiEntry, StructMember,
    Transaction, TransactionHash, TransactionOffsetInBlock, TransactionSignature,
    TransactionVersion, TypedParameter,
};
use web3::types::H160;

use crate::body::events::{
    ThinDeclareTransactionOutput, ThinDeployAccountTransactionOutput, ThinDeployTransactionOutput,
    ThinInvokeTransactionOutput, ThinL1HandlerTransactionOutput,
};
use crate::state::{IndexedDeclaredContract, IndexedDeployedContract};
use crate::test_utils::{get_test_header, read_json_file, read_json_file_from_storage_resources};
use crate::{
    EventIndex, MarkerKind, OmmerEventKey, OmmerTransactionKey, StorageSerde, ThinStateDiff,
    ThinTransactionOutput, TransactionIndex,
};

#[tokio::test]
async fn block_header() -> Result<(), anyhow::Error> {
    let item = get_test_header();
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = BlockHeader::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn block_status() -> Result<(), anyhow::Error> {
    let item = BlockStatus::default();
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = BlockStatus::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn contract_class() -> Result<(), anyhow::Error> {
    let item: ContractClass =
        serde_json::from_value(read_json_file_from_storage_resources("contract_class.json")?)?;
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = ContractClass::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn contract_class_abi_entry() -> Result<(), anyhow::Error> {
    let item = ContractClassAbiEntry::Event(EventAbiEntry::default());
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = ContractClassAbiEntry::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn declare_transaction() -> Result<(), anyhow::Error> {
    let item = DeclareTransaction {
        transaction_hash: TransactionHash(shash!("0x1")),
        max_fee: Fee(0),
        version: TransactionVersion(shash!("0x1")),
        signature: TransactionSignature(vec![shash!("0x1")]),
        nonce: Nonce::new(shash!("0x1")),
        class_hash: ClassHash::new(shash!("0x1")),
        sender_address: ContractAddress::try_from(shash!("0x1"))?,
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = DeclareTransaction::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn deploy_account_transaction() -> Result<(), anyhow::Error> {
    let item = DeployAccountTransaction {
        transaction_hash: TransactionHash(shash!("0x1")),
        max_fee: Fee(0),
        version: TransactionVersion(shash!("0x1")),
        signature: TransactionSignature(vec![shash!("0x1")]),
        nonce: Nonce::new(shash!("0x1")),
        class_hash: ClassHash::new(shash!("0x1")),
        contract_address: ContractAddress::try_from(shash!("0x1"))?,
        contract_address_salt: ContractAddressSalt(shash!("0x1")),
        constructor_calldata: CallData(vec![shash!("0x1")]),
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = DeployAccountTransaction::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn deploy_transaction() -> Result<(), anyhow::Error> {
    let item = DeployTransaction {
        transaction_hash: TransactionHash(shash!("0x1")),
        version: TransactionVersion(shash!("0x1")),
        class_hash: ClassHash::new(shash!("0x1")),
        contract_address: ContractAddress::try_from(shash!("0x1"))?,
        contract_address_salt: ContractAddressSalt(shash!("0x1")),
        constructor_calldata: CallData(vec![shash!("0x1")]),
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = DeployTransaction::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn event_abi_entry() -> Result<(), anyhow::Error> {
    let item = EventAbiEntry {
        name: "a".to_string(),
        keys: vec![TypedParameter { name: "a".to_string(), r#type: "a".to_string() }],
        data: vec![TypedParameter { name: "a".to_string(), r#type: "a".to_string() }],
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = EventAbiEntry::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn event_content() -> Result<(), anyhow::Error> {
    let item =
        EventContent { keys: vec![EventKey(shash!("0x1"))], data: EventData(vec![shash!("0x0")]) };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = EventContent::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn event_index() -> Result<(), anyhow::Error> {
    let item = EventIndex(
        TransactionIndex(BlockNumber::new(0), TransactionOffsetInBlock(0)),
        EventIndexInTransactionOutput(0),
    );
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = EventIndex::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn function_abi_entry() -> Result<(), anyhow::Error> {
    let item = FunctionAbiEntry {
        name: "a".to_string(),
        inputs: vec![TypedParameter { name: "a".to_string(), r#type: "a".to_string() }],
        outputs: vec![TypedParameter { name: "a".to_string(), r#type: "a".to_string() }],
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = FunctionAbiEntry::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn indexed_declared_contract() -> Result<(), anyhow::Error> {
    let item = IndexedDeclaredContract {
        block_number: BlockNumber::new(0),
        contract_class: serde_json::from_value(read_json_file_from_storage_resources(
            "contract_class.json",
        )?)?,
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = IndexedDeclaredContract::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn indexed_deployed_contract() -> Result<(), anyhow::Error> {
    let item = IndexedDeployedContract {
        block_number: BlockNumber::new(0),
        class_hash: ClassHash::new(shash!("0x1")),
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = IndexedDeployedContract::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn invoke_transaction() -> Result<(), anyhow::Error> {
    let item = InvokeTransaction {
        transaction_hash: TransactionHash(shash!("0x1")),
        max_fee: Fee(0),
        version: TransactionVersion(shash!("0x1")),
        signature: TransactionSignature(vec![shash!("0x1")]),
        nonce: Nonce::new(shash!("0x1")),
        contract_address: ContractAddress::try_from(shash!("0x1"))?,
        entry_point_selector: Some(EntryPointSelector(shash!("0x1"))),
        calldata: CallData(vec![shash!("0x1")]),
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = InvokeTransaction::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn l1_handler_transaction() -> Result<(), anyhow::Error> {
    let item = L1HandlerTransaction {
        transaction_hash: TransactionHash(shash!("0x1")),
        version: TransactionVersion(shash!("0x1")),
        nonce: Nonce::new(shash!("0x1")),
        contract_address: ContractAddress::try_from(shash!("0x1"))?,
        entry_point_selector: EntryPointSelector(shash!("0x1")),
        calldata: CallData(vec![shash!("0x1")]),
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = L1HandlerTransaction::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn marker_kind() -> Result<(), anyhow::Error> {
    let item = MarkerKind::Header;
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = MarkerKind::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn message_to_l2() -> Result<(), anyhow::Error> {
    let item = MessageToL2 {
        from_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
        payload: L1ToL2Payload(vec![shash!("0x1")]),
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = MessageToL2::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn program() -> Result<(), anyhow::Error> {
    let item: Program = serde_json::from_value(read_json_file("program.json")?)?;
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = Program::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn ommer_event_key() -> Result<(), anyhow::Error> {
    let item = OmmerEventKey(
        OmmerTransactionKey(BlockHash::new(shash!("0x1")), TransactionOffsetInBlock(0)),
        EventIndexInTransactionOutput(0),
    );
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = OmmerEventKey::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn storage_diff() -> Result<(), anyhow::Error> {
    let item = StorageDiff {
        address: ContractAddress::try_from(shash!("0x1"))?,
        storage_entries: vec![StorageEntry {
            key: StorageKey::try_from(shash!("0x1"))?,
            value: shash!("0x1"),
        }],
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = StorageDiff::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn struct_abi_entry() -> Result<(), anyhow::Error> {
    let item = StructAbiEntry {
        name: "a".to_string(),
        size: 1,
        members: vec![StructMember {
            param: TypedParameter { name: "a".to_string(), r#type: "a".to_string() },
            offset: 1,
        }],
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = StructAbiEntry::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn thin_declare_transaction_output() -> Result<(), anyhow::Error> {
    let item = ThinDeclareTransactionOutput {
        actual_fee: Fee(0),
        messages_sent: vec![MessageToL1 {
            to_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
            payload: L2ToL1Payload(vec![shash!("0x1")]),
        }],
        events_contract_addresses: vec![ContractAddress::try_from(shash!("0x1"))?],
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = ThinDeclareTransactionOutput::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn thin_deploy_account_transaction_output() -> Result<(), anyhow::Error> {
    let item = ThinDeployAccountTransactionOutput {
        actual_fee: Fee(0),
        messages_sent: vec![MessageToL1 {
            to_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
            payload: L2ToL1Payload(vec![shash!("0x1")]),
        }],
        events_contract_addresses: vec![ContractAddress::try_from(shash!("0x1"))?],
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = ThinDeployAccountTransactionOutput::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn thin_deploy_transaction_output() -> Result<(), anyhow::Error> {
    let item = ThinDeployTransactionOutput {
        actual_fee: Fee(0),
        messages_sent: vec![MessageToL1 {
            to_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
            payload: L2ToL1Payload(vec![shash!("0x1")]),
        }],
        events_contract_addresses: vec![ContractAddress::try_from(shash!("0x1"))?],
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = ThinDeployTransactionOutput::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn thin_invoke_transaction_output() -> Result<(), anyhow::Error> {
    let item = ThinInvokeTransactionOutput {
        actual_fee: Fee(0),
        messages_sent: vec![MessageToL1 {
            to_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
            payload: L2ToL1Payload(vec![shash!("0x1")]),
        }],
        events_contract_addresses: vec![ContractAddress::try_from(shash!("0x1"))?],
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = ThinInvokeTransactionOutput::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn thin_l1_handler_transaction_output() -> Result<(), anyhow::Error> {
    let item = ThinL1HandlerTransactionOutput {
        actual_fee: Fee(0),
        messages_sent: vec![MessageToL1 {
            to_address: EthAddress(H160(bytes_from_hex_str::<20, true>("0x1")?)),
            payload: L2ToL1Payload(vec![shash!("0x1")]),
        }],
        events_contract_addresses: vec![ContractAddress::try_from(shash!("0x1"))?],
    };
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = ThinL1HandlerTransactionOutput::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn thin_state_diff() -> Result<(), anyhow::Error> {
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
            contract_class: ContractClass::default(),
        }],
        vec![ContractNonce { contract_address: address, nonce: Nonce::new(shash!("0x1")) }],
    )?;
    let item = ThinStateDiff::from(state_diff);
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = ThinStateDiff::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn thin_transaction_output() -> Result<(), anyhow::Error> {
    let item = ThinTransactionOutput::Declare(ThinDeclareTransactionOutput::default());
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = ThinTransactionOutput::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}

#[tokio::test]
async fn transaction() -> Result<(), anyhow::Error> {
    let item = Transaction::Declare(DeclareTransaction::default());
    let mut serialized: Vec<u8> = Vec::new();
    item.serialize_into(&mut serialized)?;
    let bytes = serialized.into_boxed_slice();
    let deserialized = Transaction::deserialize_from(&mut bytes.as_ref());
    assert_eq!(item, deserialized.unwrap());

    Ok(())
}
