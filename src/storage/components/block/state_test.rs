use crate::starknet::{
    shash, BlockNumber, ClassHash, ContractAddress, DeployedContract, StarkHash, StateDiffForward,
    StorageDiff, StorageEntry, StorageKey,
};
use crate::storage::components::block::test_utils::get_test_storage;

use super::{StateStorageReader, StateStorageWriter};
#[test]
fn test_append_diff() {
    let c0 = ContractAddress(shash!("0x1"));
    let c1 = ContractAddress(shash!("0x2"));
    let c2 = ContractAddress(shash!("0x3"));
    let cl0 = ClassHash(shash!("0x4"));
    let cl1 = ClassHash(shash!("0x5"));
    let key0 = StorageKey(shash!("0x100"));
    let key1 = StorageKey(shash!("0x101"));
    let diff0 = StateDiffForward {
        deployed_contracts: vec![
            DeployedContract {
                address: c0,
                class_hash: cl0,
            },
            DeployedContract {
                address: c1,
                class_hash: cl1,
            },
        ],
        storage_diffs: vec![
            StorageDiff {
                address: c0,
                diff: vec![
                    StorageEntry {
                        addr: key0.clone(),
                        value: shash!("0x200"),
                    },
                    StorageEntry {
                        addr: key1.clone(),
                        value: shash!("0x201"),
                    },
                ],
            },
            StorageDiff {
                address: c1,
                diff: vec![],
            },
        ],
    };
    let diff1 = StateDiffForward {
        deployed_contracts: vec![DeployedContract {
            address: c2,
            class_hash: cl0,
        }],
        storage_diffs: vec![
            StorageDiff {
                address: c0,
                diff: vec![
                    StorageEntry {
                        addr: key0.clone(),
                        value: shash!("0x300"),
                    },
                    StorageEntry {
                        addr: key1.clone(),
                        value: shash!("0x0"),
                    },
                ],
            },
            StorageDiff {
                address: c1,
                diff: vec![StorageEntry {
                    addr: key0.clone(),
                    value: shash!("0x0"),
                }],
            },
        ],
    };

    let (reader, mut writer) = get_test_storage();
    let state0txn = reader.get_state_reader_txn().unwrap();
    let state0 = state0txn.get_state_reader().unwrap();
    writer.append_state_diff(BlockNumber(0), &diff0).unwrap();
    let state1txn = reader.get_state_reader_txn().unwrap();
    let state1 = state1txn.get_state_reader().unwrap();
    writer.append_state_diff(BlockNumber(1), &diff1).unwrap();
    let state2txn = reader.get_state_reader_txn().unwrap();
    let state2 = state2txn.get_state_reader().unwrap();

    // Contract0.
    assert_eq!(state0.get_class_hash_at(c0).unwrap(), None);
    assert_eq!(state1.get_class_hash_at(c0).unwrap(), Some(cl0));
    assert_eq!(state2.get_class_hash_at(c0).unwrap(), Some(cl0));

    // Contract1.
    assert_eq!(state0.get_class_hash_at(c1).unwrap(), None);
    assert_eq!(state1.get_class_hash_at(c1).unwrap(), Some(cl1));
    assert_eq!(state2.get_class_hash_at(c1).unwrap(), Some(cl1));

    // Contract2.
    assert_eq!(state0.get_class_hash_at(c2).unwrap(), None);
    assert_eq!(state1.get_class_hash_at(c2).unwrap(), None);
    assert_eq!(state2.get_class_hash_at(c2).unwrap(), Some(cl0));

    // Storage at key0.
    assert_eq!(state0.get_storage_at(c0, &key0).unwrap(), shash!("0x0"));
    assert_eq!(state1.get_storage_at(c0, &key0).unwrap(), shash!("0x200"));
    assert_eq!(state2.get_storage_at(c0, &key0).unwrap(), shash!("0x300"));

    // Storage at key1.
    assert_eq!(state0.get_storage_at(c0, &key1).unwrap(), shash!("0x0"));
    assert_eq!(state1.get_storage_at(c0, &key1).unwrap(), shash!("0x201"));
    assert_eq!(state2.get_storage_at(c0, &key1).unwrap(), shash!("0x0"));

    // Storage at key2.
    assert_eq!(state0.get_storage_at(c1, &key0).unwrap(), shash!("0x0"));
    assert_eq!(state1.get_storage_at(c1, &key0).unwrap(), shash!("0x0"));
    assert_eq!(state2.get_storage_at(c1, &key0).unwrap(), shash!("0x0"));
}
