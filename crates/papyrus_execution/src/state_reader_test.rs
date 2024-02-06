use assert_matches::assert_matches;
use blockifier::execution::contract_class::{
    ContractClass as BlockifierContractClass,
    ContractClassV0,
    ContractClassV1,
};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::StateReader;
use cairo_lang_utils::bigint::BigUintAsHex;
use indexmap::{indexmap, IndexMap};
use papyrus_common::pending_classes::{ApiContractClass, PendingClasses, PendingClassesTrait};
use papyrus_common::state::{
    DeclaredClassHashEntry,
    DeployedContract,
    ReplacedClass,
    StorageEntry,
};
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use starknet_api::block::{BlockBody, BlockHash, BlockHeader, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::patricia_key;
use starknet_api::state::{ContractClass, StateDiff, StateNumber, StorageKey};
use starknet_types_core::felt::Felt;

use crate::objects::PendingData;
use crate::state_reader::ExecutionStateReader;
use crate::test_utils::{get_test_casm, get_test_deprecated_contract_class};

const CONTRACT_ADDRESS: u64 = 0x2;
const DEPRECATED_CONTRACT_ADDRESS: u64 = 0x1;

#[test]
fn read_state() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();

    let class_hash0 = ClassHash(2u128.into());
    let address0 = ContractAddress(patricia_key!(CONTRACT_ADDRESS));
    let storage_key0 = StorageKey(patricia_key!(0x0));
    let storage_value0 = Felt::from_hex_unchecked("0x777");
    let storage_value1 = Felt::from_hex_unchecked("0x888");
    // The class is not used in the execution, so it can be default.
    let class0 = ContractClass::default();
    let casm0 = get_test_casm();
    let blockifier_casm0 =
        BlockifierContractClass::V1(ContractClassV1::try_from(casm0.clone()).unwrap());
    let compiled_class_hash0 = CompiledClassHash(Felt::default());

    let class_hash1 = ClassHash(1u128.into());
    let class1 = get_test_deprecated_contract_class();
    let address1 = ContractAddress(patricia_key!(DEPRECATED_CONTRACT_ADDRESS));
    let nonce0 = Nonce(Felt::ONE);

    let address2 = ContractAddress(patricia_key!(0x123));
    let storage_value2 = Felt::from_hex_unchecked("0x999");
    let class_hash2 = ClassHash(1234u128.into());
    let compiled_class_hash2 = CompiledClassHash(Felt::TWO);
    let mut casm1 = get_test_casm();
    casm1.bytecode[0] = BigUintAsHex { value: 12345u32.into() };
    let blockifier_casm1 =
        BlockifierContractClass::V1(ContractClassV1::try_from(casm1.clone()).unwrap());
    let nonce1 = Nonce(Felt::TWO);
    let class_hash3 = ClassHash(567_u128.into());
    let class_hash4 = ClassHash(89_u128.into());
    let class_hash5 = ClassHash(98765_u128.into());

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_state_diff(BlockNumber(0), StateDiff::default(), IndexMap::new())
        .unwrap()
        .append_header(
            BlockNumber(1),
            &BlockHeader {
                block_hash: BlockHash(Felt::ONE),
                block_number: BlockNumber(1),
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(1), BlockBody::default())
        .unwrap()
        .append_state_diff(
            BlockNumber(1),
            StateDiff {
                deployed_contracts: indexmap!(
                    address0 => class_hash0,
                    address1 => class_hash1,
                ),
                storage_diffs: indexmap!(
                    address0 => indexmap!(
                        storage_key0 => storage_value0,
                    ),
                ),
                declared_classes: indexmap!(
                    class_hash0 =>
                    (compiled_class_hash0, class0.clone()),
                    class_hash5 =>
                    (compiled_class_hash0, class0.clone())
                ),
                deprecated_declared_classes: indexmap!(
                    class_hash1 => class1.clone(),
                ),
                nonces: indexmap!(
                    address0 => nonce0,
                    address1 => Nonce::default(),
                ),
                replaced_classes: indexmap!(),
            },
            indexmap!(),
        )
        .unwrap()
        .append_casm(&class_hash0, &casm0)
        .unwrap()
        .append_header(
            BlockNumber(2),
            &BlockHeader {
                block_hash: BlockHash(Felt::TWO),
                block_number: BlockNumber(2),
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(2), BlockBody::default())
        .unwrap()
        .append_state_diff(BlockNumber(2), StateDiff::default(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let state_number0 = StateNumber::right_after_block(BlockNumber(0));
    let mut state_reader0 = ExecutionStateReader {
        storage_reader: storage_reader.clone(),
        state_number: state_number0,
        maybe_pending_data: None,
        missing_compiled_class: None,
    };
    let storage_after_block_0 = state_reader0.get_storage_at(address0, storage_key0).unwrap();
    assert_eq!(storage_after_block_0, Felt::default());
    let nonce_after_block_0 = state_reader0.get_nonce_at(address0).unwrap();
    assert_eq!(nonce_after_block_0, Nonce::default());
    let class_hash_after_block_0 = state_reader0.get_class_hash_at(address0).unwrap();
    assert_eq!(class_hash_after_block_0, ClassHash::default());
    let compiled_contract_class_after_block_0 =
        state_reader0.get_compiled_contract_class(class_hash0);
    assert_matches!(
        compiled_contract_class_after_block_0, Err(StateError::UndeclaredClassHash(class_hash))
        if class_hash == class_hash0
    );
    assert_eq!(state_reader0.get_compiled_class_hash(class_hash0).unwrap(), compiled_class_hash0);

    let state_number1 = StateNumber::right_after_block(BlockNumber(1));
    let mut state_reader1 = ExecutionStateReader {
        storage_reader: storage_reader.clone(),
        state_number: state_number1,
        maybe_pending_data: None,
        missing_compiled_class: None,
    };
    let storage_after_block_1 = state_reader1.get_storage_at(address0, storage_key0).unwrap();
    assert_eq!(storage_after_block_1, storage_value0);
    let nonce_after_block_1 = state_reader1.get_nonce_at(address0).unwrap();
    assert_eq!(nonce_after_block_1, nonce0);
    let class_hash_after_block_1 = state_reader1.get_class_hash_at(address0).unwrap();
    assert_eq!(class_hash_after_block_1, class_hash0);
    let compiled_contract_class_after_block_1 =
        state_reader1.get_compiled_contract_class(class_hash0).unwrap();
    assert_eq!(compiled_contract_class_after_block_1, blockifier_casm0);

    // Test that if we try to get a casm and it's missing, that an error is returned and the field
    // `missing_compiled_class` is set to its hash
    state_reader1.get_compiled_contract_class(class_hash5).unwrap_err();
    assert_eq!(state_reader1.missing_compiled_class.unwrap(), class_hash5);

    let state_number2 = StateNumber::right_after_block(BlockNumber(2));
    let mut state_reader2 = ExecutionStateReader {
        storage_reader,
        state_number: state_number2,
        maybe_pending_data: None,
        missing_compiled_class: None,
    };
    let nonce_after_block_2 = state_reader2.get_nonce_at(address0).unwrap();
    assert_eq!(nonce_after_block_2, nonce0);

    // Test pending state diff
    let mut pending_classes = PendingClasses::default();
    pending_classes.add_compiled_class(class_hash2, casm1);
    pending_classes.add_class(class_hash3, ApiContractClass::ContractClass(class0));
    pending_classes
        .add_class(class_hash4, ApiContractClass::DeprecatedContractClass(class1.clone()));
    state_reader2.maybe_pending_data = Some(PendingData {
        storage_diffs: indexmap!(
            address0 => vec![StorageEntry{key: storage_key0, value: storage_value1}],
            address2 => vec![StorageEntry{key: storage_key0, value: storage_value2}],
        ),
        deployed_contracts: vec![DeployedContract { address: address2, class_hash: class_hash2 }],
        declared_classes: vec![DeclaredClassHashEntry {
            class_hash: class_hash2,
            compiled_class_hash: compiled_class_hash2,
        }],
        nonces: indexmap!(
            address2 => nonce1,
        ),
        classes: pending_classes,
        ..Default::default()
    });

    assert_eq!(state_reader2.get_storage_at(address0, storage_key0).unwrap(), storage_value1);
    assert_eq!(state_reader2.get_storage_at(address2, storage_key0).unwrap(), storage_value2);
    assert_eq!(state_reader2.get_class_hash_at(address0).unwrap(), class_hash0);
    assert_eq!(state_reader2.get_class_hash_at(address2).unwrap(), class_hash2);
    assert_eq!(state_reader2.get_compiled_class_hash(class_hash0).unwrap(), compiled_class_hash0);
    assert_eq!(state_reader2.get_compiled_class_hash(class_hash2).unwrap(), compiled_class_hash2);
    assert_eq!(state_reader2.get_nonce_at(address0).unwrap(), nonce0);
    assert_eq!(state_reader2.get_nonce_at(address2).unwrap(), nonce1);
    assert_eq!(state_reader2.get_compiled_contract_class(class_hash0).unwrap(), blockifier_casm0);
    assert_eq!(state_reader2.get_compiled_contract_class(class_hash2).unwrap(), blockifier_casm1);
    // Test that if we only got the class without the casm then an error is returned.
    state_reader2.get_compiled_contract_class(class_hash3).unwrap_err();
    // Test that if the class is deprecated it is returned.
    assert_eq!(
        state_reader2.get_compiled_contract_class(class_hash4).unwrap(),
        BlockifierContractClass::V0(ContractClassV0::try_from(class1).unwrap())
    );

    // Test get_class_hash_at when the class is replaced.
    if let Some(pending_data) = &mut state_reader2.maybe_pending_data {
        pending_data.replaced_classes = vec![
            ReplacedClass { address: address0, class_hash: class_hash3 },
            ReplacedClass { address: address2, class_hash: class_hash3 },
        ];
    }
    assert_eq!(state_reader2.get_class_hash_at(address0).unwrap(), class_hash3);
    assert_eq!(state_reader2.get_class_hash_at(address2).unwrap(), class_hash3);
}

// Make sure we have the arbitrary precision feature of serde_json.
#[test]
fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}
