use std::collections::BTreeMap;

use assert_matches::assert_matches;
use logtest::Logger;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::StarkHash;
use starknet_api::state::{
    ContractClass, ContractClassAbiEntry, FunctionAbiEntry, FunctionAbiEntryType,
    FunctionAbiEntryWithType, StateDiff, StateNumber, StorageEntry, StorageKey,
};
use starknet_api::{patky, shash};

use crate::state::{StateStorageReader, StateStorageWriter, StorageError};
use crate::test_utils::{get_test_state_diff, get_test_storage};
use crate::StorageWriter;

#[test]
fn append_state_diff() -> Result<(), anyhow::Error> {
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
    let mut diff0 = StateDiff {
        deployed_contracts: BTreeMap::from([(c0, cl0), (c1, cl1)]),
        storage_diffs: BTreeMap::from([
            (
                c0,
                vec![
                    StorageEntry { key: key0, value: shash!("0x200") },
                    StorageEntry { key: key1, value: shash!("0x201") },
                ],
            ),
            (c1, vec![]),
        ]),
        declared_classes: BTreeMap::from([(cl0, c_cls0.clone()), (cl1, c_cls1)]),
        nonces: BTreeMap::from([(c0, Nonce(StarkHash::from(1)))]),
    };
    let mut diff1 = StateDiff {
        deployed_contracts: BTreeMap::from([(c2, cl0)]),
        storage_diffs: BTreeMap::from([
            (
                c0,
                vec![
                    StorageEntry { key: key0, value: shash!("0x300") },
                    StorageEntry { key: key1, value: shash!("0x0") },
                ],
            ),
            (c1, vec![StorageEntry { key: key0, value: shash!("0x0") }]),
        ]),
        declared_classes: BTreeMap::from([(cl0, c_cls0.clone())]),
        nonces: BTreeMap::from([
            (c0, Nonce(StarkHash::from(2))),
            (c1, Nonce(StarkHash::from(1))),
            (c2, Nonce(StarkHash::from(1))),
        ]),
    };

    let (_, mut writer) = get_test_storage();
    let mut txn = writer.begin_rw_txn()?;
    assert_eq!(txn.get_state_diff(BlockNumber(0))?, None);
    assert_eq!(txn.get_state_diff(BlockNumber(1))?, None);
    txn = txn.append_state_diff(BlockNumber(0), diff0.clone(), vec![])?;
    let thin_state_diff_0 = diff0.clone().into();
    assert_eq!(txn.get_state_diff(BlockNumber(0))?.unwrap(), thin_state_diff_0);
    assert_eq!(txn.get_state_diff(BlockNumber(1))?, None);
    txn = txn.append_state_diff(BlockNumber(1), diff1.clone(), vec![])?;
    let thin_state_diff_1 = diff1.clone().into();

    txn.commit()?;

    // Check for ClassAlreadyExists error when trying to declare a different class to an existing
    // class hash.
    let txn = writer.begin_rw_txn()?;
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
    let txn = writer.begin_rw_txn()?;
    let (_, hash) = diff0.deployed_contracts.iter_mut().next().unwrap();
    *hash = cl2;
    if let Err(err) = txn.append_state_diff(BlockNumber(2), diff0, vec![]) {
        assert_matches!(err, StorageError::ContractAlreadyExists { address: _ });
    } else {
        panic!("Unexpected Ok.");
    }
    let txn = writer.begin_rw_txn()?;
    assert_eq!(txn.get_state_diff(BlockNumber(0))?.unwrap(), thin_state_diff_0);
    assert_eq!(txn.get_state_diff(BlockNumber(1))?.unwrap(), thin_state_diff_1);

    let statetxn = txn.get_state_reader()?;

    // State numbers.
    let state0 = StateNumber::right_before_block(BlockNumber(0));
    let state1 = StateNumber::right_before_block(BlockNumber(1));
    let state2 = StateNumber::right_before_block(BlockNumber(2));

    // Class0.
    assert_eq!(statetxn.get_class_definition_at(state0, &cl0)?, None);
    assert_eq!(statetxn.get_class_definition_at(state1, &cl0)?, Some(c_cls0.clone()));
    assert_eq!(statetxn.get_class_definition_at(state2, &cl0)?, Some(c_cls0.clone()));

    // Class1.
    assert_eq!(statetxn.get_class_definition_at(state0, &cl1)?, None);
    assert_eq!(statetxn.get_class_definition_at(state1, &cl1)?, Some(c_cls0.clone()));
    assert_eq!(statetxn.get_class_definition_at(state2, &cl1)?, Some(c_cls0));

    // Contract0.
    assert_eq!(statetxn.get_class_hash_at(state0, &c0)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c0)?, Some(cl0));
    assert_eq!(statetxn.get_class_hash_at(state2, &c0)?, Some(cl0));
    assert_eq!(statetxn.get_nonce_at(state0, &c0)?, None);
    assert_eq!(statetxn.get_nonce_at(state1, &c0)?, Some(Nonce(StarkHash::from(1))));
    assert_eq!(statetxn.get_nonce_at(state2, &c0)?, Some(Nonce(StarkHash::from(2))));

    // Contract1.
    assert_eq!(statetxn.get_class_hash_at(state0, &c1)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c1)?, Some(cl1));
    assert_eq!(statetxn.get_class_hash_at(state2, &c1)?, Some(cl1));
    assert_eq!(statetxn.get_nonce_at(state0, &c1)?, None);
    assert_eq!(statetxn.get_nonce_at(state1, &c1)?, Some(Nonce::default()));
    assert_eq!(statetxn.get_nonce_at(state2, &c1)?, Some(Nonce(StarkHash::from(1))));

    // Contract2.
    assert_eq!(statetxn.get_class_hash_at(state0, &c2)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c2)?, None);
    assert_eq!(statetxn.get_class_hash_at(state2, &c2)?, Some(cl0));
    assert_eq!(statetxn.get_nonce_at(state0, &c2)?, None);
    assert_eq!(statetxn.get_nonce_at(state1, &c2)?, None);
    assert_eq!(statetxn.get_nonce_at(state2, &c2)?, Some(Nonce(StarkHash::from(1))));

    // Contract3.
    assert_eq!(statetxn.get_class_hash_at(state0, &c3)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c3)?, None);
    assert_eq!(statetxn.get_class_hash_at(state2, &c3)?, None);
    assert_eq!(statetxn.get_nonce_at(state0, &c3)?, None);
    assert_eq!(statetxn.get_nonce_at(state1, &c3)?, None);
    assert_eq!(statetxn.get_nonce_at(state2, &c3)?, None);

    // Storage at key0.
    assert_eq!(statetxn.get_storage_at(state0, &c0, &key0)?, shash!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c0, &key0)?, shash!("0x200"));
    assert_eq!(statetxn.get_storage_at(state2, &c0, &key0)?, shash!("0x300"));

    // Storage at key1.
    assert_eq!(statetxn.get_storage_at(state0, &c0, &key1)?, shash!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c0, &key1)?, shash!("0x201"));
    assert_eq!(statetxn.get_storage_at(state2, &c0, &key1)?, shash!("0x0"));

    // Storage at key2.
    assert_eq!(statetxn.get_storage_at(state0, &c1, &key0)?, shash!("0x0"));
    assert_eq!(statetxn.get_storage_at(state1, &c1, &key0)?, shash!("0x0"));
    assert_eq!(statetxn.get_storage_at(state2, &c1, &key0)?, shash!("0x0"));

    Ok(())
}

#[test]
fn revert_non_existing_state_diff() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();

    let mut logger = Logger::start();
    let block_number = BlockNumber(5);
    writer.begin_rw_txn()?.revert_state_diff(block_number)?;
    let expected_warn = format!(
        "Attempt to revert a non-existing state diff of block {:?}. Returning without an action.",
        block_number
    );
    assert_eq!(logger.pop().unwrap().args(), expected_warn);

    Ok(())
}

#[tokio::test]
async fn revert_last_state_diff_success() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    let (_, _, state_diff, declared_contracts) = get_test_state_diff();
    writer
        .begin_rw_txn()?
        .append_state_diff(BlockNumber(0), state_diff, declared_contracts)?
        .commit()?;

    writer.begin_rw_txn()?.revert_state_diff(BlockNumber(0))?.commit()?;
    Ok(())
}

#[tokio::test]
async fn revert_old_state_diff_fails() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    append_2_state_diffs(&mut writer)?;
    if let Err(err) = writer.begin_rw_txn()?.revert_state_diff(BlockNumber(0)) {
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
    Ok(())
}

#[tokio::test]
async fn revert_state_diff_updates_marker() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    append_2_state_diffs(&mut writer)?;

    // Verify that the state marker before revert is 2.
    assert_eq!(reader.begin_ro_txn()?.get_state_marker()?, BlockNumber(2));

    writer.begin_rw_txn()?.revert_state_diff(BlockNumber(1))?.commit()?;
    assert_eq!(reader.begin_ro_txn()?.get_state_marker()?, BlockNumber(1));

    Ok(())
}

#[tokio::test]
async fn get_reverted_state_diff_returns_none() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    append_2_state_diffs(&mut writer)?;

    // Verify that we can get block 1's state before the revert.
    assert!(reader.begin_ro_txn()?.get_state_diff(BlockNumber(1))?.is_some());

    writer.begin_rw_txn()?.revert_state_diff(BlockNumber(1))?.commit()?;
    assert!(reader.begin_ro_txn()?.get_state_diff(BlockNumber(1))?.is_none());

    Ok(())
}

fn append_2_state_diffs(writer: &mut StorageWriter) -> Result<(), anyhow::Error> {
    writer
        .begin_rw_txn()?
        .append_state_diff(BlockNumber(0), StateDiff::default(), vec![])?
        .append_state_diff(BlockNumber(1), StateDiff::default(), vec![])?
        .commit()?;

    Ok(())
}

#[test]
fn revert_doesnt_delete_previously_declared_classes() -> Result<(), anyhow::Error> {
    // Append 2 state diffs that use the same declared class.
    let c0 = ContractAddress(patky!("0x11"));
    let cl0 = ClassHash(shash!("0x4"));
    let c_cls0 = ContractClass::default();
    let diff0 = StateDiff {
        deployed_contracts: BTreeMap::from([(c0, cl0)]),
        storage_diffs: BTreeMap::new(),
        declared_classes: BTreeMap::from([(cl0, c_cls0.clone())]),
        nonces: BTreeMap::from([(c0, Nonce(StarkHash::from(1)))]),
    };

    let c1 = ContractAddress(patky!("0x12"));
    let diff1 = StateDiff {
        deployed_contracts: BTreeMap::from([(c1, cl0)]),
        storage_diffs: BTreeMap::new(),
        declared_classes: BTreeMap::from([(cl0, c_cls0)]),
        nonces: BTreeMap::from([(c1, Nonce(StarkHash::from(2)))]),
    };

    let (reader, mut writer) = get_test_storage();
    writer
        .begin_rw_txn()?
        .append_state_diff(BlockNumber(0), diff0, vec![])?
        .append_state_diff(BlockNumber(1), diff1, vec![])?
        .commit()?;

    // Assert that reverting diff 1 doesn't delete declared class from diff 0.
    writer.begin_rw_txn()?.revert_state_diff(BlockNumber(1))?.commit()?;
    let declared_class = reader
        .begin_ro_txn()?
        .get_state_reader()?
        .get_class_definition_at(StateNumber::right_after_block(BlockNumber(0)), &cl0)?;
    assert!(declared_class.is_some());

    // Assert that reverting diff 0 deletes the declared class.
    writer.begin_rw_txn()?.revert_state_diff(BlockNumber(0))?.commit()?;
    let declared_class = reader
        .begin_ro_txn()?
        .get_state_reader()?
        .get_class_definition_at(StateNumber::right_after_block(BlockNumber(0)), &cl0)?;
    assert!(declared_class.is_none());

    Ok(())
}
