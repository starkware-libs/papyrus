use assert_matches::assert_matches;
use starknet_api::{
    shash, BlockNumber, ClassHash, ContractAddress, ContractClass, DeclaredContract,
    DeployedContract, StarkHash, StateDiff, StateNumber, StorageDiff, StorageEntry, StorageKey,
};

use super::{StateStorageReader, StateStorageWriter, StorageError};
use crate::storage::test_utils::get_test_storage;

#[test]
fn test_append_diff() -> Result<(), anyhow::Error> {
    let c0 = ContractAddress(shash!("0x11"));
    let c1 = ContractAddress(shash!("0x2"));
    let c2 = ContractAddress(shash!("0x3"));
    let cl0 = ClassHash(shash!("0x4"));
    let cl1 = ClassHash(shash!("0x5"));
    let c_cls0 = DeclaredContract { class_hash: cl0, contract_class: ContractClass::default() };
    let c_cls1 = DeclaredContract { class_hash: cl1, contract_class: ContractClass::default() };
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
        declared_contracts: vec![c_cls0.class_hash, c_cls1.class_hash],
        nonces: vec![],
    };
    let diff1 = StateDiff {
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
        declared_contracts: vec![c_cls0.class_hash],
        nonces: vec![],
    };

    let (_, mut writer) = get_test_storage();
    let mut txn = writer.begin_rw_txn()?;
    assert_eq!(txn.get_state_diff(BlockNumber(0))?, None);
    assert_eq!(txn.get_state_diff(BlockNumber(1))?, None);
    txn = txn.append_state_diff(BlockNumber(0), &diff0, vec![c_cls0.clone(), c_cls1])?;
    assert_eq!(txn.get_state_diff(BlockNumber(0))?.unwrap(), diff0);
    assert_eq!(txn.get_state_diff(BlockNumber(1))?, None);
    txn = txn.append_state_diff(BlockNumber(1), &diff1, vec![c_cls0.clone()])?;
    txn.commit()?;

    // Check for ClassAlreadyExists error when trying to declare a different class to an existing
    // class hash.
    let txn = writer.begin_rw_txn()?;
    let mut class = c_cls0.clone();
    class.contract_class.abi = serde_json::Value::String("junk".to_string());
    if let Err(err) = txn.append_state_diff(BlockNumber(2), &diff1, vec![class]) {
        assert_matches!(err, StorageError::ClassAlreadyExists { class_hash: _ });
    } else {
        panic!("Unexpected Ok.");
    }

    let txn = writer.begin_rw_txn()?;
    assert_eq!(txn.get_state_diff(BlockNumber(0))?.unwrap(), diff0);
    assert_eq!(txn.get_state_diff(BlockNumber(1))?.unwrap(), diff1);

    let statetxn = txn.get_state_reader()?;

    // State numbers.
    let state0 = StateNumber::right_before_block(BlockNumber(0));
    let state1 = StateNumber::right_before_block(BlockNumber(1));
    let state2 = StateNumber::right_before_block(BlockNumber(2));

    // Class0.
    assert_eq!(statetxn.get_class_definition_at(state0, &cl0)?, None);
    assert_eq!(
        statetxn.get_class_definition_at(state1, &cl0)?,
        Some(c_cls0.contract_class.clone())
    );
    assert_eq!(
        statetxn.get_class_definition_at(state2, &cl0)?,
        Some(c_cls0.contract_class.clone())
    );

    // Class1.
    assert_eq!(statetxn.get_class_definition_at(state0, &cl1)?, None);
    assert_eq!(
        statetxn.get_class_definition_at(state1, &cl1)?,
        Some(c_cls0.contract_class.clone())
    );
    assert_eq!(statetxn.get_class_definition_at(state2, &cl1)?, Some(c_cls0.contract_class));

    // Contract0.
    assert_eq!(statetxn.get_class_hash_at(state0, &c0)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c0)?, Some(cl0));
    assert_eq!(statetxn.get_class_hash_at(state2, &c0)?, Some(cl0));

    // Contract1.
    assert_eq!(statetxn.get_class_hash_at(state0, &c1)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c1)?, Some(cl1));
    assert_eq!(statetxn.get_class_hash_at(state2, &c1)?, Some(cl1));

    // Contract2.
    assert_eq!(statetxn.get_class_hash_at(state0, &c2)?, None);
    assert_eq!(statetxn.get_class_hash_at(state1, &c2)?, None);
    assert_eq!(statetxn.get_class_hash_at(state2, &c2)?, Some(cl0));

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
