use indexmap::IndexMap;
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StateDiff, StorageKey};
use starknet_api::{patricia_key, stark_felt};

use crate::sort_state_diff;

// TODO(anatg): Add a test to check that the sync calls the sort_state_diff function
// before writing to the storage.
#[test]
fn state_sorted() {
    let hash0 = stark_felt!("0x0");
    let patricia_key0 = patricia_key!("0x0");
    let hash1 = stark_felt!("0x1");
    let patricia_key1 = patricia_key!("0x1");

    let dep_contract_0 = (ContractAddress(patricia_key0), ClassHash(hash0));
    let dep_contract_1 = (ContractAddress(patricia_key1), ClassHash(hash1));
    let storage_key_0 = StorageKey(patricia_key!("0x0"));
    let storage_key_1 = StorageKey(patricia_key!("0x1"));
    let dec_contract_0 = (ClassHash(hash0), ContractClass::default());
    let dec_contract_1 = (ClassHash(hash1), ContractClass::default());
    let nonce_0 = (ContractAddress(patricia_key0), Nonce(hash0));
    let nonce_1 = (ContractAddress(patricia_key1), Nonce(hash1));

    let unsorted_deployed_contracts = IndexMap::from([dep_contract_1, dep_contract_0]);
    let unsorted_declared_contracts =
        IndexMap::from([dec_contract_1.clone(), dec_contract_0.clone()]);
    let unsorted_nonces = IndexMap::from([nonce_1, nonce_0]);
    let unsorted_storage_entries = IndexMap::from([(storage_key_1, hash1), (storage_key_0, hash0)]);
    let unsorted_storage_diffs = IndexMap::from([
        (ContractAddress(patricia_key1), unsorted_storage_entries.clone()),
        (ContractAddress(patricia_key0), unsorted_storage_entries),
    ]);

    let mut state_diff = StateDiff {
        deployed_contracts: unsorted_deployed_contracts,
        storage_diffs: unsorted_storage_diffs,
        declared_classes: unsorted_declared_contracts,
        nonces: unsorted_nonces,
    };

    let sorted_deployed_contracts = IndexMap::from([dep_contract_0, dep_contract_1]);
    let sorted_declared_contracts = IndexMap::from([dec_contract_0, dec_contract_1]);
    let sorted_nonces = IndexMap::from([nonce_0, nonce_1]);
    let sorted_storage_entries = IndexMap::from([(storage_key_0, hash0), (storage_key_1, hash1)]);
    let sorted_storage_diffs = IndexMap::from([
        (ContractAddress(patricia_key0), sorted_storage_entries.clone()),
        (ContractAddress(patricia_key1), sorted_storage_entries.clone()),
    ]);

    sort_state_diff(&mut state_diff);
    assert_eq!(
        state_diff.deployed_contracts.get_index(0).unwrap(),
        sorted_deployed_contracts.get_index(0).unwrap(),
    );
    assert_eq!(
        state_diff.declared_classes.get_index(0).unwrap(),
        sorted_declared_contracts.get_index(0).unwrap(),
    );
    assert_eq!(
        state_diff.storage_diffs.get_index(0).unwrap(),
        sorted_storage_diffs.get_index(0).unwrap(),
    );
    assert_eq!(
        state_diff.storage_diffs.get_index(0).unwrap().1.get_index(0).unwrap(),
        sorted_storage_entries.get_index(0).unwrap(),
    );
    assert_eq!(state_diff.nonces.get_index(0).unwrap(), sorted_nonces.get_index(0).unwrap());
}
