use blockifier::execution::contract_class::{ContractClassV0, ContractClassV1};
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader;
use blockifier::test_utils::{get_deprecated_contract_class, TEST_CONTRACT_PATH};
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use indexmap::{indexmap, IndexMap};
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::test_utils::get_test_storage;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StateDiff, StorageKey};
use starknet_api::{patricia_key, stark_felt};
use test_utils::read_json_file;

use crate::blockifier_state::{PapyrusReader, PapyrusStateReader};

#[test]
fn class_hash_nonce_and_storage_at() {
    let c0 = ContractAddress(patricia_key!("0x11"));
    let c1 = ContractAddress(patricia_key!("0x12"));
    let c2 = ContractAddress(patricia_key!("0x13"));
    let c3 = ContractAddress(patricia_key!("0x14"));
    let cl0 = ClassHash(stark_felt!("0x4"));
    let cl1 = ClassHash(stark_felt!("0x5"));
    let c_cls0 = DeprecatedContractClass::default();
    let c_cls1 = (CompiledClassHash::default(), ContractClass::default());
    let key0 = StorageKey(patricia_key!("0x1001"));
    let key1 = StorageKey(patricia_key!("0x101"));
    let nc0_diff0 = Nonce(StarkHash::from(1_u8));
    let nc0_diff1 = Nonce(StarkHash::from(2_u8));
    let nc1_diff1 = Nonce(StarkHash::from(1_u8));
    let nc2_diff1 = Nonce(StarkHash::from(1_u8));
    let diff0 = StateDiff {
        deployed_contracts: IndexMap::from([(c0, cl0), (c1, cl1)]),
        storage_diffs: IndexMap::from([
            (c0, IndexMap::from([(key0, stark_felt!("0x200")), (key1, stark_felt!("0x201"))])),
            (c1, IndexMap::new()),
        ]),
        deprecated_declared_classes: IndexMap::from([(cl0, c_cls0.clone())]),
        declared_classes: IndexMap::from([(cl1, c_cls1)]),
        nonces: IndexMap::from([(c0, nc0_diff0)]),
        replaced_classes: indexmap! {},
    };
    let diff1 = StateDiff {
        deployed_contracts: IndexMap::from([(c2, cl0)]),
        storage_diffs: IndexMap::from([
            (c0, IndexMap::from([(key0, stark_felt!("0x300")), (key1, stark_felt!("0x0"))])),
            (c1, IndexMap::from([(key0, stark_felt!("0x0"))])),
        ]),
        deprecated_declared_classes: IndexMap::from([(cl0, c_cls0)]),
        declared_classes: indexmap! {},
        nonces: IndexMap::from([(c0, nc0_diff1), (c1, nc1_diff1), (c2, nc2_diff1)]),
        replaced_classes: IndexMap::from([(c0, cl1)]),
    };

    // Write state diffs.
    let (storage_reader, mut writer) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_state_diff(BlockNumber(0), diff0, IndexMap::new()).unwrap();
    txn = txn.append_state_diff(BlockNumber(1), diff1, IndexMap::new()).unwrap();
    txn.commit().unwrap();

    // Before state diff 0.
    let casm_reader_txn = storage_reader.begin_ro_txn().unwrap();
    let txn = storage_reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let block_number = BlockNumber(0);
    let fixed_block_state_reader = PapyrusStateReader::new(state_reader, block_number);
    let papyrus_reader = PapyrusReader::new(&casm_reader_txn, fixed_block_state_reader);
    let mut state = CachedState::new(papyrus_reader);

    assert_eq!(state.get_class_hash_at(c0).unwrap(), ClassHash::default());
    assert_eq!(state.get_class_hash_at(c1).unwrap(), ClassHash::default());
    assert_eq!(state.get_class_hash_at(c2).unwrap(), ClassHash::default());
    assert_eq!(state.get_class_hash_at(c3).unwrap(), ClassHash::default());
    assert_eq!(state.get_nonce_at(c0).unwrap(), Nonce::default());
    assert_eq!(state.get_nonce_at(c1).unwrap(), Nonce::default());
    assert_eq!(state.get_nonce_at(c2).unwrap(), Nonce::default());
    assert_eq!(state.get_nonce_at(c3).unwrap(), Nonce::default());
    assert_eq!(state.get_storage_at(c0, key0).unwrap(), StarkFelt::default());
    assert_eq!(state.get_storage_at(c0, key1).unwrap(), StarkFelt::default());
    assert_eq!(state.get_storage_at(c1, key0).unwrap(), StarkFelt::default());

    // After state diff 0.
    let casm_reader_txn = storage_reader.begin_ro_txn().unwrap();
    let txn = storage_reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let block_number = BlockNumber(1);
    let fixed_block_state_reader = PapyrusStateReader::new(state_reader, block_number);
    let papyrus_reader = PapyrusReader::new(&casm_reader_txn, fixed_block_state_reader);
    let mut state = CachedState::new(papyrus_reader);

    assert_eq!(state.get_class_hash_at(c0).unwrap(), cl0);
    assert_eq!(state.get_class_hash_at(c1).unwrap(), cl1);
    assert_eq!(state.get_class_hash_at(c2).unwrap(), ClassHash::default());
    assert_eq!(state.get_class_hash_at(c3).unwrap(), ClassHash::default());
    assert_eq!(state.get_nonce_at(c0).unwrap(), nc0_diff0);
    assert_eq!(state.get_nonce_at(c1).unwrap(), Nonce::default());
    assert_eq!(state.get_nonce_at(c2).unwrap(), Nonce::default());
    assert_eq!(state.get_nonce_at(c3).unwrap(), Nonce::default());
    assert_eq!(state.get_storage_at(c0, key0).unwrap(), stark_felt!("0x200"));
    assert_eq!(state.get_storage_at(c0, key1).unwrap(), stark_felt!("0x201"));
    assert_eq!(state.get_storage_at(c1, key0).unwrap(), stark_felt!("0x0"));

    // After state diff 1.
    let casm_reader_txn = storage_reader.begin_ro_txn().unwrap();
    let txn = storage_reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let block_number = BlockNumber(2);
    let fixed_block_state_reader = PapyrusStateReader::new(state_reader, block_number);
    let papyrus_reader = PapyrusReader::new(&casm_reader_txn, fixed_block_state_reader);
    let mut state = CachedState::new(papyrus_reader);

    assert_eq!(state.get_class_hash_at(c0).unwrap(), cl1);
    assert_eq!(state.get_class_hash_at(c1).unwrap(), cl1);
    assert_eq!(state.get_class_hash_at(c2).unwrap(), cl0);
    assert_eq!(state.get_class_hash_at(c3).unwrap(), ClassHash::default());
    assert_eq!(state.get_nonce_at(c0).unwrap(), nc0_diff1);
    assert_eq!(state.get_nonce_at(c1).unwrap(), nc1_diff1);
    assert_eq!(state.get_nonce_at(c2).unwrap(), nc2_diff1);
    assert_eq!(state.get_nonce_at(c3).unwrap(), Nonce::default());
    assert_eq!(state.get_storage_at(c0, key0).unwrap(), stark_felt!("0x300"));
    assert_eq!(state.get_storage_at(c0, key1).unwrap(), stark_felt!("0x0"));
    assert_eq!(state.get_storage_at(c1, key0).unwrap(), stark_felt!("0x0"));
}

#[test]
fn compiled_class() {
    let contract_0 = ContractAddress(patricia_key!("0x00"));
    let contract_1 = ContractAddress(patricia_key!("0x01"));
    let dep_class = get_deprecated_contract_class(TEST_CONTRACT_PATH);
    let new_class = (CompiledClassHash::default(), ContractClass::default());
    let hash_0 = ClassHash(stark_felt!("0x10"));
    let hash_1 = ClassHash(stark_felt!("0x11"));
    let diff0 = StateDiff {
        deployed_contracts: IndexMap::from([(contract_0, hash_0), (contract_1, hash_1)]),
        deprecated_declared_classes: IndexMap::from([(hash_0, dep_class.clone())]),
        declared_classes: IndexMap::from([(hash_1, new_class)]),
        ..Default::default()
    };
    let casm_json = read_json_file("compiled_class.json");
    let compiled_class: CasmContractClass = serde_json::from_value(casm_json).unwrap();

    // Write state diff and compiled class.
    let (storage_reader, mut writer) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_state_diff(BlockNumber(0), diff0, IndexMap::new()).unwrap();
    txn = txn.append_casm(hash_1, &compiled_class).unwrap();
    txn.commit().unwrap();

    // Create and test state reader.
    let casm_reader_txn = storage_reader.begin_ro_txn().unwrap();
    let txn = storage_reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let block_number = BlockNumber(1);
    let fixed_block_state_reader = PapyrusStateReader::new(state_reader, block_number);
    let papyrus_reader = PapyrusReader::new(&casm_reader_txn, fixed_block_state_reader);
    let mut state = CachedState::new(papyrus_reader);

    assert_eq!(
        state.get_compiled_contract_class(&hash_0).unwrap(),
        ContractClassV0::try_from(dep_class).unwrap().into()
    );
    assert_eq!(
        state.get_compiled_contract_class(&hash_1).unwrap(),
        ContractClassV1::try_from(compiled_class).unwrap().into()
    );
}
