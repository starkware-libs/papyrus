use assert_matches::assert_matches;
use indexmap::{indexmap, IndexMap};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass, ContractClassAbiEntry, FunctionAbiEntry,
    FunctionAbiEntryType, FunctionAbiEntryWithType,
};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StateDiff, StateNumber, StorageKey};
use starknet_api::{patricia_key, stark_felt};
use test_utils::get_test_state_diff;

use crate::state::{StateStorageReader, StateStorageWriter, StorageError};
use crate::test_utils::get_test_storage;
use crate::{StorageWriter, ThinStateDiff};

#[test]
fn append_state_diff_declared_classes() {
    // Deprecated classes.
    let dc0 = ClassHash(stark_felt!("0x00"));
    let dc1 = ClassHash(stark_felt!("0x01"));
    let dep_class = DeprecatedContractClass::default();
    // New classes.
    let nc0 = ClassHash(stark_felt!("0x10"));
    let nc1 = ClassHash(stark_felt!("0x11"));
    let new_class = (CompiledClassHash::default(), ContractClass::default());
    let diff0 = StateDiff {
        deprecated_declared_classes: IndexMap::from([
            (dc0, dep_class.clone()),
            (dc1, dep_class.clone()),
        ]),
        declared_classes: IndexMap::from([(nc0, new_class.clone())]),
        ..Default::default()
    };
    let diff1 = StateDiff {
        deprecated_declared_classes: IndexMap::from([(dc0, dep_class.clone())]),
        declared_classes: IndexMap::from([(nc1, new_class.clone())]),
        ..Default::default()
    };

    let (_, mut writer) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_state_diff(BlockNumber(0), diff0, IndexMap::new()).unwrap();
    txn = txn.append_state_diff(BlockNumber(1), diff1.clone(), IndexMap::new()).unwrap();
    txn.commit().unwrap();

    // State numbers.
    let state0 = StateNumber::right_before_block(BlockNumber(0));
    let state1 = StateNumber::right_before_block(BlockNumber(1));
    let state2 = StateNumber::right_before_block(BlockNumber(2));

    // ___Deprecated Classes Test___
    // Check for ClassAlreadyExists error when trying to declare another class to an existing
    // class hash.
    let txn = writer.begin_rw_txn().unwrap();
    let mut diff2 = StateDiff {
        deprecated_declared_classes: diff1.deprecated_declared_classes,
        ..StateDiff::default()
    };
    let (_, class) = diff2.deprecated_declared_classes.iter_mut().next().unwrap();
    class.abi = Some(vec![ContractClassAbiEntry::Function(FunctionAbiEntryWithType {
        r#type: FunctionAbiEntryType::Regular,
        entry: FunctionAbiEntry { name: String::from("junk"), inputs: vec![], outputs: vec![] },
    })]);
    if let Err(err) = txn.append_state_diff(BlockNumber(2), diff2, IndexMap::new()) {
        assert_matches!(err, StorageError::ClassAlreadyExists { class_hash: _ });
    } else {
        panic!("Unexpected Ok.");
    }

    let txn = writer.begin_rw_txn().unwrap();
    let statetxn = txn.get_state_reader().unwrap();

    // Class0.
    assert_matches!(statetxn.get_deprecated_class_definition_at(state0, &dc0).unwrap(), None);
    assert_matches!(statetxn.get_deprecated_class_definition_at(state1, &dc0).unwrap(), Some(_));
    assert_matches!(statetxn.get_deprecated_class_definition_at(state2, &dc0).unwrap(), Some(_));

    // Class1.
    assert_matches!(statetxn.get_deprecated_class_definition_at(state0, &dc1).unwrap(), None);
    assert_matches!(statetxn.get_deprecated_class_definition_at(state1, &dc1).unwrap(), Some(_));
    assert_matches!(statetxn.get_deprecated_class_definition_at(state2, &dc1).unwrap(), Some(_));

    // ___New Classes Test___
    // Check for ClassAlreadyExists error when trying to declare a different class to an existing
    // class hash.
    drop(txn);
    let txn = writer.begin_rw_txn().unwrap();
    let diff2 = StateDiff { declared_classes: diff1.declared_classes, ..StateDiff::default() };
    if let Err(err) = txn.append_state_diff(BlockNumber(2), diff2, IndexMap::new()) {
        assert_matches!(err, StorageError::ClassAlreadyExists { class_hash: _ });
    } else {
        panic!("Unexpected Ok.");
    }

    let txn = writer.begin_rw_txn().unwrap();
    let statetxn = txn.get_state_reader().unwrap();

    // Class0.
    assert_matches!(statetxn.get_class_definition_at(state0, &nc0).unwrap(), None);
    assert_matches!(statetxn.get_class_definition_at(state1, &nc0).unwrap(), Some(_));
    assert_matches!(statetxn.get_class_definition_at(state2, &nc0).unwrap(), Some(_));

    // Class1.
    assert_matches!(statetxn.get_class_definition_at(state0, &nc1).unwrap(), None);
    assert_matches!(statetxn.get_class_definition_at(state1, &nc1).unwrap(), None);
    assert_matches!(statetxn.get_class_definition_at(state2, &nc1).unwrap(), Some(_));

    // Check for ClassAlreadyExists error when trying to declare another class using the new
    // version to an existing class hash in the older version .
    drop(txn);
    let txn = writer.begin_rw_txn().unwrap();
    let diff2 =
        StateDiff { declared_classes: IndexMap::from([(dc0, new_class)]), ..StateDiff::default() };
    if let Err(err) = txn.append_state_diff(BlockNumber(2), diff2, IndexMap::new()) {
        assert_matches!(err, StorageError::ClassAlreadyExists { class_hash: _ });
    } else {
        panic!("Unexpected Ok.");
    }

    // Check for ClassAlreadyExists error when trying to declare another class using the old
    // version to an existing class hash in the new version.
    let txn = writer.begin_rw_txn().unwrap();
    let diff2 = StateDiff {
        deprecated_declared_classes: IndexMap::from([(nc0, dep_class)]),
        ..StateDiff::default()
    };
    if let Err(err) = txn.append_state_diff(BlockNumber(2), diff2, IndexMap::new()) {
        assert_matches!(err, StorageError::ClassAlreadyExists { class_hash: _ });
    } else {
        panic!("Unexpected Ok.");
    };
}

#[test]
fn append_state_diff() {
    // TODO(dvir): Add declared_classes.
    // TODO(dvir): Add replaced_classes.
    let c0 = ContractAddress(patricia_key!("0x11"));
    let c1 = ContractAddress(patricia_key!("0x12"));
    let c2 = ContractAddress(patricia_key!("0x13"));
    let c3 = ContractAddress(patricia_key!("0x14"));
    let cl0 = ClassHash(stark_felt!("0x4"));
    let cl1 = ClassHash(stark_felt!("0x5"));
    let cl2 = ClassHash(stark_felt!("0x6"));
    let c_cls0 = DeprecatedContractClass::default();
    let c_cls1 = DeprecatedContractClass::default();
    let key0 = StorageKey(patricia_key!("0x1001"));
    let key1 = StorageKey(patricia_key!("0x101"));
    let diff0 = StateDiff {
        deployed_contracts: IndexMap::from([(c0, cl0), (c1, cl1)]),
        storage_diffs: IndexMap::from([
            (c0, IndexMap::from([(key0, stark_felt!("0x200")), (key1, stark_felt!("0x201"))])),
            (c1, IndexMap::new()),
        ]),
        deprecated_declared_classes: IndexMap::from([(cl0, c_cls0.clone()), (cl1, c_cls1)]),
        declared_classes: indexmap! {},
        nonces: IndexMap::from([(c0, Nonce(StarkHash::from(1)))]),
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
        nonces: IndexMap::from([
            (c0, Nonce(StarkHash::from(2))),
            (c1, Nonce(StarkHash::from(1))),
            (c2, Nonce(StarkHash::from(1))),
        ]),
        replaced_classes: indexmap! {},
    };

    let (_, mut writer) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    assert_eq!(txn.get_state_diff(BlockNumber(0)).unwrap(), None);
    assert_eq!(txn.get_state_diff(BlockNumber(1)).unwrap(), None);
    txn = txn.append_state_diff(BlockNumber(0), diff0.clone(), IndexMap::new()).unwrap();
    let thin_state_diff_0 = diff0.clone().into();
    assert_eq!(txn.get_state_diff(BlockNumber(0)).unwrap().unwrap(), thin_state_diff_0);
    assert_eq!(txn.get_state_diff(BlockNumber(1)).unwrap(), None);
    txn = txn.append_state_diff(BlockNumber(1), diff1.clone(), IndexMap::new()).unwrap();
    let thin_state_diff_1 = diff1.into();

    txn.commit().unwrap();

    // Check for ContractAlreadyExists error when trying to deploy a different class hash to an
    // existing contract address.
    let txn = writer.begin_rw_txn().unwrap();
    let mut diff2 =
        StateDiff { deployed_contracts: diff0.deployed_contracts, ..StateDiff::default() };
    let (_, hash) = diff2.deployed_contracts.iter_mut().next().unwrap();
    *hash = cl2;
    if let Err(err) = txn.append_state_diff(BlockNumber(2), diff2, IndexMap::new()) {
        assert_matches!(err, StorageError::ContractAlreadyExists { address: _ });
    } else {
        panic!("Unexpected Ok.");
    }
    let txn = writer.begin_rw_txn().unwrap();
    assert_eq!(txn.get_state_diff(BlockNumber(0)).unwrap().unwrap(), thin_state_diff_0);
    assert_eq!(txn.get_state_diff(BlockNumber(1)).unwrap().unwrap(), thin_state_diff_1);

    let statetxn = txn.get_state_reader().unwrap();

    // State numbers.
    let state0 = StateNumber::right_before_block(BlockNumber(0));
    let state1 = StateNumber::right_before_block(BlockNumber(1));
    let state2 = StateNumber::right_before_block(BlockNumber(2));

    // Contract0.
    assert_eq!(statetxn.get_class_hash_at(state0, &c0).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c0).unwrap(), Some(cl0));
    assert_eq!(statetxn.get_class_hash_at(state2, &c0).unwrap(), Some(cl0));
    assert_eq!(statetxn.get_nonce_at(state0, &c0).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state1, &c0).unwrap(), Some(Nonce(StarkHash::from(1))));
    assert_eq!(statetxn.get_nonce_at(state2, &c0).unwrap(), Some(Nonce(StarkHash::from(2))));

    // Contract1.
    assert_eq!(statetxn.get_class_hash_at(state0, &c1).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c1).unwrap(), Some(cl1));
    assert_eq!(statetxn.get_class_hash_at(state2, &c1).unwrap(), Some(cl1));
    assert_eq!(statetxn.get_nonce_at(state0, &c1).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state1, &c1).unwrap(), Some(Nonce::default()));
    assert_eq!(statetxn.get_nonce_at(state2, &c1).unwrap(), Some(Nonce(StarkHash::from(1))));

    // Contract2.
    assert_eq!(statetxn.get_class_hash_at(state0, &c2).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c2).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state2, &c2).unwrap(), Some(cl0));
    assert_eq!(statetxn.get_nonce_at(state0, &c2).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state1, &c2).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state2, &c2).unwrap(), Some(Nonce(StarkHash::from(1))));

    // Contract3.
    assert_eq!(statetxn.get_class_hash_at(state0, &c3).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c3).unwrap(), None);
    assert_eq!(statetxn.get_class_hash_at(state2, &c3).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state0, &c3).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state1, &c3).unwrap(), None);
    assert_eq!(statetxn.get_nonce_at(state2, &c3).unwrap(), None);

    // Storage at key0.
    assert_eq!(statetxn.get_storage_at(state0, &c0, &key0).unwrap(), stark_felt!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c0, &key0).unwrap(), stark_felt!("0x200"));
    assert_eq!(statetxn.get_storage_at(state2, &c0, &key0).unwrap(), stark_felt!("0x300"));

    // Storage at key1.
    assert_eq!(statetxn.get_storage_at(state0, &c0, &key1).unwrap(), stark_felt!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c0, &key1).unwrap(), stark_felt!("0x201"));
    assert_eq!(statetxn.get_storage_at(state2, &c0, &key1).unwrap(), stark_felt!("0x0"));

    // Storage at key2.
    assert_eq!(statetxn.get_storage_at(state0, &c1, &key0).unwrap(), stark_felt!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c1, &key0).unwrap(), stark_felt!("0x0"));
    assert_eq!(statetxn.get_storage_at(state2, &c1, &key0).unwrap(), stark_felt!("0x0"));
}

#[test]
fn revert_non_existing_state_diff() {
    let (_, mut writer) = get_test_storage();

    let block_number = BlockNumber(5);
    let (_, deleted_data) = writer.begin_rw_txn().unwrap().revert_state_diff(block_number).unwrap();
    assert!(deleted_data.is_none());
}

#[tokio::test]
async fn revert_last_state_diff_success() {
    let (_, mut writer) = get_test_storage();
    let state_diff = get_test_state_diff();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), state_diff, IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap();
    txn.commit().unwrap();
}

#[tokio::test]
async fn revert_old_state_diff_fails() {
    let (_, mut writer) = get_test_storage();
    append_2_state_diffs(&mut writer);
    let (_, deleted_data) =
        writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap();
    assert!(deleted_data.is_none());
}

#[tokio::test]
async fn revert_state_diff_updates_marker() {
    let (reader, mut writer) = get_test_storage();
    append_2_state_diffs(&mut writer);

    // Verify that the state marker before revert is 2.
    assert_eq!(reader.begin_ro_txn().unwrap().get_state_marker().unwrap(), BlockNumber(2));

    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap();
    txn.commit().unwrap();
    assert_eq!(reader.begin_ro_txn().unwrap().get_state_marker().unwrap(), BlockNumber(1));
}

#[tokio::test]
async fn get_reverted_state_diff_returns_none() {
    let (reader, mut writer) = get_test_storage();
    append_2_state_diffs(&mut writer);

    // Verify that we can get block 1's state before the revert.
    assert!(reader.begin_ro_txn().unwrap().get_state_diff(BlockNumber(1)).unwrap().is_some());

    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap();
    txn.commit().unwrap();
    assert!(reader.begin_ro_txn().unwrap().get_state_diff(BlockNumber(1)).unwrap().is_none());
}

fn append_2_state_diffs(writer: &mut StorageWriter) {
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), StateDiff::default(), IndexMap::new())
        .unwrap()
        .append_state_diff(BlockNumber(1), StateDiff::default(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();
}

#[test]
fn revert_doesnt_delete_previously_declared_classes() {
    // Append 2 state diffs that use the same declared class.
    // TODO(dvir): Add declared_classes.
    let c0 = ContractAddress(patricia_key!("0x11"));
    let cl0 = ClassHash(stark_felt!("0x4"));
    let c_cls0 = DeprecatedContractClass::default();
    let diff0 = StateDiff {
        deployed_contracts: IndexMap::from([(c0, cl0)]),
        storage_diffs: IndexMap::new(),
        deprecated_declared_classes: IndexMap::from([(cl0, c_cls0.clone())]),
        declared_classes: indexmap! {},
        nonces: IndexMap::from([(c0, Nonce(StarkHash::from(1)))]),
        replaced_classes: indexmap! {},
    };

    let c1 = ContractAddress(patricia_key!("0x12"));
    let diff1 = StateDiff {
        deployed_contracts: IndexMap::from([(c1, cl0)]),
        storage_diffs: IndexMap::new(),
        deprecated_declared_classes: IndexMap::from([(cl0, c_cls0)]),
        declared_classes: indexmap! {},
        nonces: IndexMap::from([(c1, Nonce(StarkHash::from(2)))]),
        replaced_classes: indexmap! {},
    };

    let (reader, mut writer) = get_test_storage();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0, IndexMap::new())
        .unwrap()
        .append_state_diff(BlockNumber(1), diff1, IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    // Assert that reverting diff 1 doesn't delete declared class from diff 0.
    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap();
    txn.commit().unwrap();
    let declared_class = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_reader()
        .unwrap()
        .get_deprecated_class_definition_at(StateNumber::right_after_block(BlockNumber(0)), &cl0)
        .unwrap();
    assert!(declared_class.is_some());

    // Assert that reverting diff 0 deletes the declared class.
    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap();
    txn.commit().unwrap();
    let declared_class = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_reader()
        .unwrap()
        .get_deprecated_class_definition_at(StateNumber::right_after_block(BlockNumber(0)), &cl0)
        .unwrap();
    assert!(declared_class.is_none());
}

#[test]
fn revert_state() {
    let state_diff0 = get_test_state_diff();
    let (contract0, class0) = state_diff0.deployed_contracts.first().unwrap();
    let (_contract0, nonce0) = state_diff0.nonces.first().unwrap();

    // TODO(dvir): Add declared_classes.
    // TODO(dvir): Add replaced_classes.
    // Create another state diff, deploying new contracts and changing the state of the contract
    // deployed in state0.
    let contract1 = ContractAddress(patricia_key!("0x111"));
    let class1 = ClassHash(stark_felt!("0x111"));
    let updated_storage_key = StorageKey(patricia_key!("0x111"));
    let new_data = StarkFelt::from(111);
    let updated_storage = IndexMap::from([(updated_storage_key, new_data)]);
    let nonce1 = Nonce(StarkFelt::from(111));
    let state_diff1 = StateDiff {
        deployed_contracts: IndexMap::from([(contract1, class1)]),
        storage_diffs: IndexMap::from([(*contract0, updated_storage)]),
        deprecated_declared_classes: IndexMap::from([(class1, DeprecatedContractClass::default())]),
        declared_classes: indexmap! {},
        nonces: IndexMap::from([(contract1, nonce1)]),
        replaced_classes: indexmap! {},
    };

    let (reader, mut writer) = get_test_storage();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), state_diff0.clone(), IndexMap::new())
        .unwrap()
        .append_state_diff(BlockNumber(1), state_diff1.clone(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    assert_eq!(txn.get_state_marker().unwrap(), BlockNumber(2));
    assert!(txn.get_state_diff(BlockNumber(1)).unwrap().is_some());

    let state_reader = txn.get_state_reader().unwrap();
    let state_number = StateNumber::right_after_block(BlockNumber(1));
    assert_eq!(state_reader.get_class_hash_at(state_number, contract0).unwrap().unwrap(), *class0);
    assert_eq!(state_reader.get_class_hash_at(state_number, &contract1).unwrap().unwrap(), class1);
    assert_eq!(state_reader.get_nonce_at(state_number, contract0).unwrap().unwrap(), *nonce0);
    assert_eq!(state_reader.get_nonce_at(state_number, &contract1).unwrap().unwrap(), nonce1);
    assert_eq!(
        state_reader.get_storage_at(state_number, contract0, &updated_storage_key).unwrap(),
        new_data
    );

    let (txn, deleted_data) =
        writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap();
    txn.commit().unwrap();

    let expected_deleted_state_diff = ThinStateDiff::from(state_diff1);
    let expected_deleted_classes = IndexMap::from([(class1, DeprecatedContractClass::default())]);
    assert_matches!(
        deleted_data,
        Some((thin_state_diff, _class_definitions, deprecated_class_definitions))
        if thin_state_diff == expected_deleted_state_diff
        && deprecated_class_definitions == expected_deleted_classes
    );

    let txn = reader.begin_ro_txn().unwrap();
    assert_eq!(txn.get_state_marker().unwrap(), BlockNumber(1));
    assert!(txn.get_state_diff(BlockNumber(1)).unwrap().is_none());

    let state_reader = txn.get_state_reader().unwrap();
    let state_number = StateNumber::right_after_block(BlockNumber(0));
    assert_eq!(state_reader.get_class_hash_at(state_number, contract0).unwrap().unwrap(), *class0);
    assert!(state_reader.get_class_hash_at(state_number, &contract1).unwrap().is_none());
    assert_eq!(state_reader.get_nonce_at(state_number, contract0).unwrap().unwrap(), *nonce0);
    assert!(state_reader.get_nonce_at(state_number, &contract1).unwrap().is_none());
    assert_eq!(
        state_reader.get_storage_at(state_number, contract0, &updated_storage_key).unwrap(),
        StarkFelt::from(0)
    );
}

#[test]
fn get_nonce_key_serialization() {
    let (reader, mut writer) = get_test_storage();
    let contract_address = ContractAddress(patricia_key!("0x11"));

    for block_number in 0..(1 << 8) + 1 {
        let state_diff = StateDiff {
            deployed_contracts: IndexMap::new(),
            storage_diffs: IndexMap::new(),
            declared_classes: IndexMap::new(),
            deprecated_declared_classes: IndexMap::new(),
            nonces: IndexMap::from([(contract_address, Nonce(StarkHash::from(block_number + 1)))]),
            replaced_classes: IndexMap::new(),
        };

        writer
            .begin_rw_txn()
            .unwrap()
            .append_state_diff(BlockNumber(block_number), state_diff, IndexMap::new())
            .unwrap()
            .commit()
            .unwrap();
    }

    let txn = reader.begin_ro_txn().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    // No nonce in genesis.
    assert_eq!(
        state_reader
            .get_nonce_at(StateNumber::right_before_block(BlockNumber(0)), &contract_address)
            .unwrap(),
        None
    );

    for block_number in 1..(1 << 8) + 1 {
        println!("{block_number:?}");
        let nonce = state_reader
            .get_nonce_at(
                StateNumber::right_before_block(BlockNumber(block_number)),
                &contract_address,
            )
            .unwrap();
        println!("{nonce:?}");
        let nonce = nonce.unwrap();

        assert_eq!(nonce, Nonce(StarkHash::from(block_number)));
    }
}

#[test]
fn replace_class() {
    let (reader, mut writer) = get_test_storage();
    let contract_address = ContractAddress(patricia_key!("0x0"));

    let class_hash0 = ClassHash(stark_felt!("0x0"));
    let state_diff1 = StateDiff {
        deployed_contracts: indexmap! {
            contract_address => class_hash0
        },
        storage_diffs: IndexMap::new(),
        declared_classes: IndexMap::new(),
        deprecated_declared_classes: indexmap! {
            class_hash0 => DeprecatedContractClass::default()
        },
        nonces: IndexMap::new(),
        replaced_classes: IndexMap::new(),
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), state_diff1, IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let state1 = StateNumber(BlockNumber(1));
    let current_class_hash = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_reader()
        .unwrap()
        .get_class_hash_at(state1, &contract_address)
        .unwrap()
        .unwrap();

    assert_eq!(current_class_hash, class_hash0);

    let class_hash1 = ClassHash(stark_felt!("0x1"));
    let state_diff2 = StateDiff {
        deployed_contracts: IndexMap::new(),
        storage_diffs: IndexMap::new(),
        declared_classes: indexmap! {
            class_hash1 => (CompiledClassHash::default(), ContractClass::default()),
        },
        deprecated_declared_classes: IndexMap::new(),
        nonces: IndexMap::new(),
        replaced_classes: indexmap! {
            contract_address => class_hash1,
        },
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(1), state_diff2, IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    // Verify that the contract class was replaced.
    let state2 = StateNumber(BlockNumber(2));
    let replaced_class_hash = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_reader()
        .unwrap()
        .get_replaced_class_hash(state2, &contract_address)
        .unwrap()
        .unwrap();

    assert_eq!(replaced_class_hash, class_hash1);

    // Verify that fetching the class hash returns the new class.
    let current_class_hash = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_reader()
        .unwrap()
        .get_class_hash_at(state2, &contract_address)
        .unwrap()
        .unwrap();

    assert_eq!(current_class_hash, class_hash1);

    // Verify that fetching the class hash from an old state returns the old class.
    let current_class_hash = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_reader()
        .unwrap()
        .get_class_hash_at(state1, &contract_address)
        .unwrap()
        .unwrap();

    assert_eq!(current_class_hash, class_hash0);
}
