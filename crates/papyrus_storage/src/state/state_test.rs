use assert_matches::assert_matches;
use logtest::Logger;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::StarkHash;
use starknet_api::state::{
    ContractClass, ContractClassAbiEntry, ContractNonce, DeclaredContract, DeployedContract,
    FunctionAbiEntry, FunctionAbiEntryType, FunctionAbiEntryWithType, StateDiff, StateNumber,
    StorageDiff, StorageEntry, StorageKey,
};
use starknet_api::{patky, shash};

use crate::state::{StateStorageReader, StateStorageWriter, StorageError};
use crate::test_utils::{get_test_state_diff, get_test_storage};
use crate::StorageWriter;

#[test]
fn append_state_diff() {
    let c0 = ContractAddress(patky!("0x11"));
    let c1 = ContractAddress(patky!("0x12"));
    let c2 = ContractAddress(patky!("0x13"));
    let c3 = ContractAddress(patky!("0x14"));
    let cl0 = ClassHash(shash!("0x4"));
    let cl1 = ClassHash(shash!("0x5"));
    let cl2 = ClassHash(shash!("0x6"));
    let c_cls0 = ContractClass::default();
    let c_cls1 = ContractClass::default();
    let key0 = StorageKey(patky!("0x1001"));
    let key1 = StorageKey(patky!("0x101"));
    let diff0 = StateDiff::new(
        vec![
            DeployedContract { address: c0, class_hash: cl0 },
            DeployedContract { address: c1, class_hash: cl1 },
        ],
        vec![
            StorageDiff::new(
                c0,
                vec![
                    StorageEntry { key: key0, value: shash!("0x200") },
                    StorageEntry { key: key1, value: shash!("0x201") },
                ],
            )
            .unwrap(),
            StorageDiff::new(c1, vec![]).unwrap(),
        ],
        vec![
            DeclaredContract { class_hash: cl0, contract_class: c_cls0.clone() },
            DeclaredContract { class_hash: cl1, contract_class: c_cls1 },
        ],
        vec![ContractNonce { contract_address: c0, nonce: Nonce(StarkHash::from(1)) }],
    )
    .unwrap();
    let diff1 = StateDiff::new(
        vec![DeployedContract { address: c2, class_hash: cl0 }],
        vec![
            StorageDiff::new(
                c0,
                vec![
                    StorageEntry { key: key0, value: shash!("0x300") },
                    StorageEntry { key: key1, value: shash!("0x0") },
                ],
            )
            .unwrap(),
            StorageDiff::new(c1, vec![StorageEntry { key: key0, value: shash!("0x0") }]).unwrap(),
        ],
        vec![DeclaredContract { class_hash: cl0, contract_class: c_cls0.clone() }],
        vec![
            ContractNonce { contract_address: c0, nonce: Nonce(StarkHash::from(2)) },
            ContractNonce { contract_address: c1, nonce: Nonce(StarkHash::from(1)) },
            ContractNonce { contract_address: c2, nonce: Nonce(StarkHash::from(1)) },
        ],
    )
    .unwrap();

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
    let (deployed_contracts, storage_diffs, mut declared_classes, nonces) = diff1.into();
    let mut class = declared_classes[0].contract_class.clone();
    class.abi = Some(vec![ContractClassAbiEntry::Function(FunctionAbiEntryWithType {
        r#type: FunctionAbiEntryType::Regular,
        entry: FunctionAbiEntry { name: String::from("junk"), inputs: vec![], outputs: vec![] },
    })]);

    declared_classes[0].contract_class = class;
    let diff1 =
        StateDiff::new(deployed_contracts, storage_diffs, declared_classes, nonces).unwrap();

    if let Err(err) = txn.append_state_diff(BlockNumber(2), diff1, vec![]) {
        assert_matches!(err, StorageError::ClassAlreadyExists { class_hash: _ });
    } else {
        panic!("Unexpected Ok.");
    }
    // Check for ContractAlreadyExists error when trying to deploy a different class hash to an
    // existing contract address.
    let txn = writer.begin_rw_txn().unwrap();
    let (mut deployed_contracts, storage_diffs, declared_classes, nonces) = diff0.into();
    let mut contract = deployed_contracts[0].clone();
    contract.class_hash = cl2;
    deployed_contracts[0] = contract;
    let diff0 =
        StateDiff::new(deployed_contracts, storage_diffs, declared_classes, nonces).unwrap();
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
    assert_eq!(statetxn.get_storage_at(state0, &c0, &key0).unwrap(), shash!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c0, &key0).unwrap(), shash!("0x200"));
    assert_eq!(statetxn.get_storage_at(state2, &c0, &key0).unwrap(), shash!("0x300"));

    // Storage at key1.
    assert_eq!(statetxn.get_storage_at(state0, &c0, &key1).unwrap(), shash!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c0, &key1).unwrap(), shash!("0x201"));
    assert_eq!(statetxn.get_storage_at(state2, &c0, &key1).unwrap(), shash!("0x0"));

    // Storage at key2.
    assert_eq!(statetxn.get_storage_at(state0, &c1, &key0).unwrap(), shash!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c1, &key0).unwrap(), shash!("0x0"));
    assert_eq!(statetxn.get_storage_at(state2, &c1, &key0).unwrap(), shash!("0x0"));
}

#[test]
fn revert_non_existing_state_diff() {
    let (_, mut writer) = get_test_storage();

    let mut logger = Logger::start();
    let block_number = BlockNumber(5);
    writer.begin_rw_txn().unwrap().revert_state_diff(block_number).unwrap();
    let expected_warn = format!(
        "Attempt to revert a non-existing state diff of block {:?}. Returning without an action.",
        block_number
    );
    assert_eq!(logger.pop().unwrap().args(), expected_warn);
}

#[tokio::test]
async fn revert_last_state_diff_success() {
    let (_, mut writer) = get_test_storage();
    let (_, _, state_diff, declared_contracts) = get_test_state_diff();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), state_diff, declared_contracts)
        .unwrap()
        .commit()
        .unwrap();

    writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap().commit().unwrap();
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

    writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap().commit().unwrap();
    assert_eq!(reader.begin_ro_txn().unwrap().get_state_marker().unwrap(), BlockNumber(1));
}

#[tokio::test]
async fn get_reverted_state_diff_returns_none() {
    let (reader, mut writer) = get_test_storage();
    append_2_state_diffs(&mut writer);

    // Verify that we can get block 1's state before the revert.
    assert!(reader.begin_ro_txn().unwrap().get_state_diff(BlockNumber(1)).unwrap().is_some());

    writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap().commit().unwrap();
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
    let c0 = ContractAddress(patky!("0x11"));
    let cl0 = ClassHash(shash!("0x4"));
    let c_cls0 = ContractClass::default();
    let diff0 = StateDiff::new(
        vec![DeployedContract { address: c0, class_hash: cl0 }],
        vec![],
        vec![DeclaredContract { class_hash: cl0, contract_class: c_cls0.clone() }],
        vec![ContractNonce { contract_address: c0, nonce: Nonce(StarkHash::from(1)) }],
    )
    .unwrap();

    let c1 = ContractAddress(patky!("0x12"));
    let diff1 = StateDiff::new(
        vec![DeployedContract { address: c1, class_hash: cl0 }],
        vec![],
        vec![DeclaredContract { class_hash: cl0, contract_class: c_cls0 }],
        vec![ContractNonce { contract_address: c1, nonce: Nonce(StarkHash::from(2)) }],
    )
    .unwrap();

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
    writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(1)).unwrap().commit().unwrap();
    let declared_class = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_reader()
        .unwrap()
        .get_class_definition_at(StateNumber::right_after_block(BlockNumber(0)), &cl0)
        .unwrap();
    assert!(declared_class.is_some());

    // Assert that reverting diff 0 deletes the declared class.
    writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(0)).unwrap().commit().unwrap();
    let declared_class = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_reader()
        .unwrap()
        .get_class_definition_at(StateNumber::right_after_block(BlockNumber(0)), &cl0)
        .unwrap();
    assert!(declared_class.is_none());
}
