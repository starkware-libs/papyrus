use assert_matches::assert_matches;
use logtest::Logger;
use starknet_api::{
    shash, BlockNumber, ClassHash, ContractAddress, ContractClass, ContractNonce, DeclaredContract,
    DeployedContract, Nonce, StarkHash, StateDiff, StateNumber, StorageDiff, StorageEntry,
    StorageKey,
};

use super::{StateStorageReader, StateStorageWriter, StorageError};
use crate::test_utils::{get_test_state_diff, get_test_storage};
use crate::StorageWriter;

#[test]
fn append_state_diff() -> Result<(), anyhow::Error> {
    let c0 = ContractAddress::try_from(shash!("0x11")).unwrap();
    let c1 = ContractAddress::try_from(shash!("0x12")).unwrap();
    let c2 = ContractAddress::try_from(shash!("0x13")).unwrap();
    let c3 = ContractAddress::try_from(shash!("0x14")).unwrap();
    let cl0 = ClassHash::new(shash!("0x4"));
    let cl1 = ClassHash::new(shash!("0x5"));
    let cl2 = ClassHash::new(shash!("0x6"));
    let c_cls0 = ContractClass::default();
    let c_cls1 = ContractClass::default();
    let key0 = StorageKey::try_from(shash!("0x1001")).unwrap();
    let key1 = StorageKey::try_from(shash!("0x101")).unwrap();
    let diff0 = StateDiff::new(
        vec![
            DeployedContract { address: c0, class_hash: cl0 },
            DeployedContract { address: c1, class_hash: cl1 },
        ],
        vec![
            StorageDiff {
                address: c0,
                storage_entries: vec![
                    StorageEntry { key: key0.clone(), value: shash!("0x200") },
                    StorageEntry { key: key1.clone(), value: shash!("0x201") },
                ],
            },
            StorageDiff { address: c1, storage_entries: vec![] },
        ],
        vec![
            DeclaredContract { class_hash: cl0, contract_class: c_cls0.clone() },
            DeclaredContract { class_hash: cl1, contract_class: c_cls1 },
        ],
        vec![ContractNonce { contract_address: c0, nonce: Nonce::new(StarkHash::from_u64(1)) }],
    )?;
    let diff1 = StateDiff::new(
        vec![DeployedContract { address: c2, class_hash: cl0 }],
        vec![
            StorageDiff {
                address: c0,
                storage_entries: vec![
                    StorageEntry { key: key0.clone(), value: shash!("0x300") },
                    StorageEntry { key: key1.clone(), value: shash!("0x0") },
                ],
            },
            StorageDiff {
                address: c1,
                storage_entries: vec![StorageEntry { key: key0.clone(), value: shash!("0x0") }],
            },
        ],
        vec![DeclaredContract { class_hash: cl0, contract_class: c_cls0.clone() }],
        vec![
            ContractNonce { contract_address: c0, nonce: Nonce::new(StarkHash::from_u64(2)) },
            ContractNonce { contract_address: c1, nonce: Nonce::new(StarkHash::from_u64(1)) },
            ContractNonce { contract_address: c2, nonce: Nonce::new(StarkHash::from_u64(1)) },
        ],
    )?;

    let (_, mut writer) = get_test_storage();
    let mut txn = writer.begin_rw_txn()?;
    assert_eq!(txn.get_state_diff(BlockNumber::new(0))?, None);
    assert_eq!(txn.get_state_diff(BlockNumber::new(1))?, None);
    txn = txn.append_state_diff(BlockNumber::new(0), diff0.clone(), vec![])?;
    let thin_state_diff_0 = diff0.clone().into();
    assert_eq!(txn.get_state_diff(BlockNumber::new(0))?.unwrap(), thin_state_diff_0);
    assert_eq!(txn.get_state_diff(BlockNumber::new(1))?, None);
    txn = txn.append_state_diff(BlockNumber::new(1), diff1.clone(), vec![])?;
    let thin_state_diff_1 = diff1.clone().into();

    txn.commit()?;

    // Check for ClassAlreadyExists error when trying to declare a different class to an existing
    // class hash.
    let txn = writer.begin_rw_txn()?;
    let (deployed_contracts, storage_diffs, mut declared_classes, nonces) = diff1.destruct();
    let mut class = declared_classes[0].contract_class.clone();
    class.abi = serde_json::Value::String("junk".to_string());

    declared_classes[0].contract_class = class;
    let diff1 = StateDiff::new(deployed_contracts, storage_diffs, declared_classes, nonces)?;

    if let Err(err) = txn.append_state_diff(BlockNumber::new(2), diff1, vec![]) {
        assert_matches!(err, StorageError::ClassAlreadyExists { class_hash: _ });
    } else {
        panic!("Unexpected Ok.");
    }
    // Check for ContractAlreadyExists error when trying to deploy a different class hash to an
    // existing contract address.
    let txn = writer.begin_rw_txn()?;
    let (mut deployed_contracts, storage_diffs, declared_classes, nonces) = diff0.destruct();
    let mut contract = deployed_contracts[0].clone();
    contract.class_hash = cl2;
    deployed_contracts[0] = contract;
    let diff0 = StateDiff::new(deployed_contracts, storage_diffs, declared_classes, nonces)?;
    if let Err(err) = txn.append_state_diff(BlockNumber::new(2), diff0, vec![]) {
        assert_matches!(err, StorageError::ContractAlreadyExists { address: _ });
    } else {
        panic!("Unexpected Ok.");
    }
    let txn = writer.begin_rw_txn()?;
    assert_eq!(txn.get_state_diff(BlockNumber::new(0))?.unwrap(), thin_state_diff_0);
    assert_eq!(txn.get_state_diff(BlockNumber::new(1))?.unwrap(), thin_state_diff_1);

    let statetxn = txn.get_state_reader()?;

    // State numbers.
    let state0 = StateNumber::right_before_block(BlockNumber::new(0));
    let state1 = StateNumber::right_before_block(BlockNumber::new(1));
    let state2 = StateNumber::right_before_block(BlockNumber::new(2));

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
    assert_eq!(statetxn.get_nonce_at(state1, &c0)?, Some(Nonce::new(StarkHash::from_u64(1))));
    assert_eq!(statetxn.get_nonce_at(state2, &c0)?, Some(Nonce::new(StarkHash::from_u64(2))));

    // Contract1.
    assert_eq!(statetxn.get_class_hash_at(state0, &c1)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c1)?, Some(cl1));
    assert_eq!(statetxn.get_class_hash_at(state2, &c1)?, Some(cl1));
    assert_eq!(statetxn.get_nonce_at(state0, &c1)?, None);
    assert_eq!(statetxn.get_nonce_at(state1, &c1)?, Some(Nonce::default()));
    assert_eq!(statetxn.get_nonce_at(state2, &c1)?, Some(Nonce::new(StarkHash::from_u64(1))));

    // Contract2.
    assert_eq!(statetxn.get_class_hash_at(state0, &c2)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c2)?, None);
    assert_eq!(statetxn.get_class_hash_at(state2, &c2)?, Some(cl0));
    assert_eq!(statetxn.get_nonce_at(state0, &c2)?, None);
    assert_eq!(statetxn.get_nonce_at(state1, &c2)?, None);
    assert_eq!(statetxn.get_nonce_at(state2, &c2)?, Some(Nonce::new(StarkHash::from_u64(1))));

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
    let block_number = BlockNumber::new(5);
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
        .append_state_diff(BlockNumber::new(0), state_diff, declared_contracts)?
        .commit()?;

    writer.begin_rw_txn()?.revert_state_diff(BlockNumber::new(0))?.commit()?;
    Ok(())
}

#[tokio::test]
async fn revert_old_state_diff_fails() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    append_2_state_diffs(&mut writer)?;
    if let Err(err) = writer.begin_rw_txn()?.revert_state_diff(BlockNumber::new(0)) {
        assert_matches!(
            err,
            StorageError::InvalidRevert {
                revert_block_number,
                block_number_marker
            }
            if revert_block_number == BlockNumber::new(0) && block_number_marker == BlockNumber::new(2)
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
    assert_eq!(reader.begin_ro_txn()?.get_state_marker()?, BlockNumber::new(2));

    writer.begin_rw_txn()?.revert_state_diff(BlockNumber::new(1))?.commit()?;
    assert_eq!(reader.begin_ro_txn()?.get_state_marker()?, BlockNumber::new(1));

    Ok(())
}

#[tokio::test]
async fn get_reverted_state_diff_returns_none() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    append_2_state_diffs(&mut writer)?;

    // Verify that we can get block 1's state before the revert.
    assert!(reader.begin_ro_txn()?.get_state_diff(BlockNumber::new(1))?.is_some());

    writer.begin_rw_txn()?.revert_state_diff(BlockNumber::new(1))?.commit()?;
    assert!(reader.begin_ro_txn()?.get_state_diff(BlockNumber::new(1))?.is_none());

    Ok(())
}

fn append_2_state_diffs(writer: &mut StorageWriter) -> Result<(), anyhow::Error> {
    writer
        .begin_rw_txn()?
        .append_state_diff(BlockNumber::new(0), StateDiff::default(), vec![])?
        .append_state_diff(BlockNumber::new(1), StateDiff::default(), vec![])?
        .commit()?;

    Ok(())
}

#[test]
fn revert_doesnt_delete_previously_declared_classes() -> Result<(), anyhow::Error> {
    // Append 2 state diffs that use the same declared class.
    let c0 = ContractAddress::try_from(shash!("0x11")).unwrap();
    let cl0 = ClassHash::new(shash!("0x4"));
    let c_cls0 = ContractClass::default();
    let diff0 = StateDiff::new(
        vec![DeployedContract { address: c0, class_hash: cl0 }],
        vec![],
        vec![DeclaredContract { class_hash: cl0, contract_class: c_cls0.clone() }],
        vec![ContractNonce { contract_address: c0, nonce: Nonce::new(StarkHash::from_u64(1)) }],
    )?;

    let c1 = ContractAddress::try_from(shash!("0x12")).unwrap();
    let diff1 = StateDiff::new(
        vec![DeployedContract { address: c1, class_hash: cl0 }],
        vec![],
        vec![DeclaredContract { class_hash: cl0, contract_class: c_cls0 }],
        vec![ContractNonce { contract_address: c1, nonce: Nonce::new(StarkHash::from_u64(2)) }],
    )?;

    let (reader, mut writer) = get_test_storage();
    writer
        .begin_rw_txn()?
        .append_state_diff(BlockNumber::new(0), diff0, vec![])?
        .append_state_diff(BlockNumber::new(1), diff1, vec![])?
        .commit()?;

    // Assert that reverting diff 1 doesn't delete declared class from diff 0.
    writer.begin_rw_txn()?.revert_state_diff(BlockNumber::new(1))?.commit()?;
    let declared_class = reader
        .begin_ro_txn()?
        .get_state_reader()?
        .get_class_definition_at(StateNumber::right_after_block(BlockNumber::new(0)), &cl0)?;
    assert!(declared_class.is_some());

    // Assert that reverting diff 0 deletes the declared class.
    writer.begin_rw_txn()?.revert_state_diff(BlockNumber::new(0))?.commit()?;
    let declared_class = reader
        .begin_ro_txn()?
        .get_state_reader()?
        .get_class_definition_at(StateNumber::right_after_block(BlockNumber::new(0)), &cl0)?;
    assert!(declared_class.is_none());

    Ok(())
}
