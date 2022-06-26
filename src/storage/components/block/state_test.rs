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
                        key: key0.clone(),
                        value: shash!("0x200"),
                    },
                    StorageEntry {
                        key: key1.clone(),
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
                        key: key0.clone(),
                        value: shash!("0x300"),
                    },
                    StorageEntry {
                        key: key1,
                        value: shash!("0x0"),
                    },
                ],
            },
            StorageDiff {
                address: c1,
                diff: vec![StorageEntry {
                    key: key0,
                    value: shash!("0x0"),
                }],
            },
        ],
    };

    let (reader, mut writer) = get_test_storage();
    assert_eq!(reader.get_state_diff(BlockNumber(0)).unwrap(), None);
    assert_eq!(reader.get_state_diff(BlockNumber(1)).unwrap(), None);
    writer.append_state_diff(BlockNumber(0), &diff0).unwrap();
    assert_eq!(
        reader.get_state_diff(BlockNumber(0)).unwrap().unwrap(),
        diff0
    );
    assert_eq!(reader.get_state_diff(BlockNumber(1)).unwrap(), None);
    writer.append_state_diff(BlockNumber(1), &diff1).unwrap();
    assert_eq!(
        reader.get_state_diff(BlockNumber(0)).unwrap().unwrap(),
        diff0
    );
    assert_eq!(
        reader.get_state_diff(BlockNumber(1)).unwrap().unwrap(),
        diff1
    );
}
