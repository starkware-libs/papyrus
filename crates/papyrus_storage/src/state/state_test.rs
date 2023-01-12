use assert_matches::assert_matches;
use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{
    ContractClass, ContractClassAbiEntry, FunctionAbiEntry, FunctionAbiEntryType,
    FunctionAbiEntryWithType, StateDiff, StateNumber, StorageKey,
};
use starknet_api::{patricia_key, stark_felt};
use test_utils::get_test_state_diff;

use crate::state::{StateStorageReader, StateStorageWriter, StorageError};
use crate::test_utils::get_test_storage;
use crate::StorageWriter;

#[test]
fn append_state_diff() {
    let c0 = ContractAddress(patricia_key!("0x11"));
    let c1 = ContractAddress(patricia_key!("0x12"));
    let c2 = ContractAddress(patricia_key!("0x13"));
    let c3 = ContractAddress(patricia_key!("0x14"));
    let cl0 = ClassHash(stark_felt!("0x4"));
    let cl1 = ClassHash(stark_felt!("0x5"));
    let cl2 = ClassHash(stark_felt!("0x6"));
    let c_cls0 = ContractClass::default();
    let c_cls1 = ContractClass::default();
    let key0 = StorageKey(patricia_key!("0x1001"));
    let key1 = StorageKey(patricia_key!("0x101"));
    let mut diff0 = StateDiff {
        deployed_contracts: IndexMap::from([(c0, cl0), (c1, cl1)]),
        storage_diffs: IndexMap::from([
            (c0, IndexMap::from([(key0, stark_felt!("0x200")), (key1, stark_felt!("0x201"))])),
            (c1, IndexMap::new()),
        ]),
        declared_classes: IndexMap::from([(cl0, c_cls0.clone()), (cl1, c_cls1)]),
        nonces: IndexMap::from([(c0, Nonce(StarkHash::from(1)))]),
    };
    let mut diff1 = StateDiff {
        deployed_contracts: IndexMap::from([(c2, cl0)]),
        storage_diffs: IndexMap::from([
            (c0, IndexMap::from([(key0, stark_felt!("0x300")), (key1, stark_felt!("0x0"))])),
            (c1, IndexMap::from([(key0, stark_felt!("0x0"))])),
        ]),
        declared_classes: IndexMap::from([(cl0, c_cls0.clone())]),
        nonces: IndexMap::from([
            (c0, Nonce(StarkHash::from(2))),
            (c1, Nonce(StarkHash::from(1))),
            (c2, Nonce(StarkHash::from(1))),
        ]),
    };

    let (_, mut writer) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    assert_eq!(txn.get_state_diff(BlockNumber(0)).unwrap(), None);
    assert_eq!(txn.get_state_diff(BlockNumber(1)).unwrap(), None);
    txn = txn.append_state_diff(BlockNumber(0), diff0.clone(), vec![]).unwrap();
    let thin_state_diff_0 = diff0.clone().into();
    assert_eq!(txn.get_state_diff(BlockNumber(0)).unwrap().unwrap(), thin_state_diff_0);
    assert_eq!(txn.get_state_diff(BlockNumber(1)).unwrap(), None);
    txn = txn.append_state_diff(BlockNumber(1), diff1.clone(), vec![]).unwrap();
    let thin_state_diff_1 = diff1.clone().into();

    txn.commit().unwrap();

    // Check for ClassAlreadyExists error when trying to declare a different class to an existing
    // class hash.
    let txn = writer.begin_rw_txn().unwrap();
    let (_, class) = diff1.declared_classes.iter_mut().next().unwrap();
    class.abi = Some(vec![ContractClassAbiEntry::Function(FunctionAbiEntryWithType {
        r#type: FunctionAbiEntryType::Regular,
        entry: FunctionAbiEntry { name: String::from("junk"), inputs: vec![], outputs: vec![] },
    })]);
    if let Err(err) = txn.append_state_diff(BlockNumber(2), diff1, vec![]) {
        assert_matches!(err, StorageError::ClassAlreadyExists { class_hash: _ });
    } else {
        panic!("Unexpected Ok.");
    }

    // Check for ContractAlreadyExists error when trying to deploy a different class hash to an
    // existing contract address.
    let txn = writer.begin_rw_txn().unwrap();
    let (_, hash) = diff0.deployed_contracts.iter_mut().next().unwrap();
    *hash = cl2;
    if let Err(err) = txn.append_state_diff(BlockNumber(2), diff0, vec![]) {
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

    // Class0.
    assert_eq!(statetxn.get_class_definition_at(state0, &cl0).unwrap(), None);
    assert_eq!(statetxn.get_class_definition_at(state1, &cl0).unwrap(), Some(c_cls0.clone()));
    assert_eq!(statetxn.get_class_definition_at(state2, &cl0).unwrap(), Some(c_cls0.clone()));

    // Class1.
    assert_eq!(statetxn.get_class_definition_at(state0, &cl1).unwrap(), None);
    assert_eq!(statetxn.get_class_definition_at(state1, &cl1).unwrap(), Some(c_cls0.clone()));
    assert_eq!(statetxn.get_class_definition_at(state2, &cl1).unwrap(), Some(c_cls0));

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
        .append_state_diff(BlockNumber(0), state_diff, vec![])
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
    let res = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0));
    if let Err(err) = res {
        assert_matches!(
            err,
            StorageError::InvalidRevert {
                revert_block_number,
                block_number_marker
            }
            if revert_block_number == BlockNumber(0) && block_number_marker == BlockNumber(2)
        );
    } else {
        panic!("Unexpected Ok.");
    }
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
        .append_state_diff(BlockNumber(0), StateDiff::default(), vec![])
        .unwrap()
        .append_state_diff(BlockNumber(1), StateDiff::default(), vec![])
        .unwrap()
        .commit()
        .unwrap();
}

#[test]
fn revert_doesnt_delete_previously_declared_classes() {
    // Append 2 state diffs that use the same declared class.
    let c0 = ContractAddress(patricia_key!("0x11"));
    let cl0 = ClassHash(stark_felt!("0x4"));
    let c_cls0 = ContractClass::default();
    let diff0 = StateDiff {
        deployed_contracts: IndexMap::from([(c0, cl0)]),
        storage_diffs: IndexMap::new(),
        declared_classes: IndexMap::from([(cl0, c_cls0.clone())]),
        nonces: IndexMap::from([(c0, Nonce(StarkHash::from(1)))]),
    };

    let c1 = ContractAddress(patricia_key!("0x12"));
    let diff1 = StateDiff {
        deployed_contracts: IndexMap::from([(c1, cl0)]),
        storage_diffs: IndexMap::new(),
        declared_classes: IndexMap::from([(cl0, c_cls0)]),
        nonces: IndexMap::from([(c1, Nonce(StarkHash::from(2)))]),
    };

    let (reader, mut writer) = get_test_storage();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), diff0, vec![])
        .unwrap()
        .append_state_diff(BlockNumber(1), diff1, vec![])
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
        .get_class_definition_at(StateNumber::right_after_block(BlockNumber(0)), &cl0)
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
        .get_class_definition_at(StateNumber::right_after_block(BlockNumber(0)), &cl0)
        .unwrap();
    assert!(declared_class.is_none());
}
