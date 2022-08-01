use assert_matches::assert_matches;
use starknet_api::{
    shash, BlockNumber, ClassHash, ContractAddress, ContractClass, DeployedContract, Nonce,
    StarkHash, StateDiff, StateNumber, StorageDiff, StorageEntry, StorageKey,
};

use super::{StateStorageReader, StateStorageWriter, StorageError};
use crate::state::split_diff_for_storage;
use crate::test_utils::get_test_storage;

#[test]
fn test_append_diff() -> Result<(), anyhow::Error> {
    let c0 = ContractAddress(shash!("0x11"));
    let c1 = ContractAddress(shash!("0x2"));
    let c2 = ContractAddress(shash!("0x3"));
    let c3 = ContractAddress(shash!("0x4"));
    let cl0 = ClassHash(shash!("0x4"));
    let cl1 = ClassHash(shash!("0x5"));
    let c_cls0 = ContractClass::default();
    let c_cls1 = ContractClass::default();
    let key0 = StorageKey(shash!("0x1001"));
    let key1 = StorageKey(shash!("0x101"));
    let diff0 = StateDiff {
        deployed_contracts: vec![
            DeployedContract { address: c0, class_hash: cl0 },
            DeployedContract { address: c1, class_hash: cl1 },
        ],
        storage_diffs: vec![
            StorageDiff {
                address: c0,
                diff: vec![
                    StorageEntry { key: key0.clone(), value: shash!("0x200") },
                    StorageEntry { key: key1.clone(), value: shash!("0x201") },
                ],
            },
            StorageDiff { address: c1, diff: vec![] },
        ],
        declared_classes: vec![(cl0, c_cls0.clone()), (cl1, c_cls1)],
        nonces: vec![
            (c0, Nonce(StarkHash::from_u64(1))),
            (c1, Nonce(StarkHash::from_u64(1))),
            (c3, Nonce(StarkHash::from_u64(1))),
        ],
    };
    let mut diff1 = StateDiff {
        deployed_contracts: vec![DeployedContract { address: c2, class_hash: cl0 }],
        storage_diffs: vec![
            StorageDiff {
                address: c0,
                diff: vec![
                    StorageEntry { key: key0.clone(), value: shash!("0x300") },
                    StorageEntry { key: key1.clone(), value: shash!("0x0") },
                ],
            },
            StorageDiff {
                address: c1,
                diff: vec![StorageEntry { key: key0.clone(), value: shash!("0x0") }],
            },
        ],
        declared_classes: vec![(cl0, c_cls0.clone())],
        nonces: vec![
            (c0, Nonce(StarkHash::from_u64(2))),
            (c1, Nonce(StarkHash::from_u64(2))),
            (c2, Nonce(StarkHash::from_u64(1))),
        ],
    };

    let (_, mut writer) = get_test_storage();
    let mut txn = writer.begin_rw_txn()?;
    assert_eq!(txn.get_state_diff(BlockNumber(0))?, None);
    assert_eq!(txn.get_state_diff(BlockNumber(1))?, None);
    txn = txn.append_state_diff(BlockNumber(0), diff0.clone())?;
    let (thin_state_diff_0, _declared_classes_0) = split_diff_for_storage(diff0);
    assert_eq!(txn.get_state_diff(BlockNumber(0))?.unwrap(), thin_state_diff_0);
    assert_eq!(txn.get_state_diff(BlockNumber(1))?, None);
    txn = txn.append_state_diff(BlockNumber(1), diff1.clone())?;
    let (this_state_diff_1, _declared_classes_1) = split_diff_for_storage(diff1.clone());
    txn.commit()?;

    // Check for ClassAlreadyExists error when trying to declare a different class to an existing
    // class hash.
    let txn = writer.begin_rw_txn()?;
    let mut class = diff1.declared_classes[0].1.clone();
    class.abi = serde_json::Value::String("junk".to_string());
    diff1.declared_classes[0].1 = class;
    if let Err(err) = txn.append_state_diff(BlockNumber(2), diff1) {
        assert_matches!(err, StorageError::ClassAlreadyExists { class_hash: _ });
    } else {
        panic!("Unexpected Ok.");
    }
    let txn = writer.begin_rw_txn()?;
    assert_eq!(txn.get_state_diff(BlockNumber(0))?.unwrap(), thin_state_diff_0);
    assert_eq!(txn.get_state_diff(BlockNumber(1))?.unwrap(), this_state_diff_1);

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
    assert_eq!(statetxn.get_nonce_at(state0, &c0)?, Nonce::default());
    assert_eq!(statetxn.get_nonce_at(state1, &c0)?, Nonce(StarkHash::from_u64(1)));
    assert_eq!(statetxn.get_nonce_at(state2, &c0)?, Nonce(StarkHash::from_u64(2)));

    // Contract1.
    assert_eq!(statetxn.get_class_hash_at(state0, &c1)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c1)?, Some(cl1));
    assert_eq!(statetxn.get_class_hash_at(state2, &c1)?, Some(cl1));
    assert_eq!(statetxn.get_nonce_at(state0, &c1)?, Nonce::default());
    assert_eq!(statetxn.get_nonce_at(state1, &c1)?, Nonce(StarkHash::from_u64(1)));
    assert_eq!(statetxn.get_nonce_at(state2, &c1)?, Nonce(StarkHash::from_u64(2)));

    // Contract2.
    assert_eq!(statetxn.get_class_hash_at(state0, &c2)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c2)?, None);
    assert_eq!(statetxn.get_class_hash_at(state2, &c2)?, Some(cl0));
    assert_eq!(statetxn.get_nonce_at(state0, &c2)?, Nonce::default());
    assert_eq!(statetxn.get_nonce_at(state1, &c2)?, Nonce::default());
    assert_eq!(statetxn.get_nonce_at(state2, &c2)?, Nonce(StarkHash::from_u64(1)));

    // Contract3.
    assert_eq!(statetxn.get_class_hash_at(state0, &c3)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c3)?, None);
    assert_eq!(statetxn.get_class_hash_at(state2, &c3)?, None);
    assert_eq!(statetxn.get_nonce_at(state0, &c3)?, Nonce::default());
    assert_eq!(statetxn.get_nonce_at(state1, &c3)?, Nonce(StarkHash::from_u64(1)));
    assert_eq!(statetxn.get_nonce_at(state2, &c3)?, Nonce(StarkHash::from_u64(1)));

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
