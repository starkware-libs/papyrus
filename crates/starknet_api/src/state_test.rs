use super::StateDiff;
use crate::{
    shash, ClassHash, ContractAddress, ContractClass, DeployedContract, Nonce, StarkHash,
    StorageDiff,
};

#[test]
fn test_sorted() {
    let hash0 = shash!("0x0");
    let hash1 = shash!("0x1");

    let dec_contract_0 =
        DeployedContract { address: ContractAddress(hash0), class_hash: ClassHash(hash0) };
    let dec_contract_1 =
        DeployedContract { address: ContractAddress(hash1), class_hash: ClassHash(hash1) };

    let storage_diff_0 = StorageDiff { address: ContractAddress(hash0), storage_entries: vec![] };
    let storage_diff_1 = StorageDiff { address: ContractAddress(hash1), storage_entries: vec![] };

    let dec_class_0 = (ClassHash(hash0), ContractClass::default());
    let dec_class_1 = (ClassHash(hash1), ContractClass::default());

    let dec_nonce_0 = (ContractAddress(hash0), Nonce(hash0));
    let dec_nonce_1 = (ContractAddress(hash1), Nonce(hash1));

    let unsorted_deployed_contracts = vec![dec_contract_1.clone(), dec_contract_0.clone()];
    let unsorted_storage_diffs = vec![storage_diff_1.clone(), storage_diff_0.clone()];
    let unsorted_declared_classes = vec![dec_class_1.clone(), dec_class_0.clone()];
    let unsorted_nonces = vec![dec_nonce_1, dec_nonce_0];

    let state_diff = StateDiff::new(
        unsorted_deployed_contracts,
        unsorted_storage_diffs,
        unsorted_declared_classes,
        unsorted_nonces,
    );

    let sorted_deployed_contracts = vec![dec_contract_0, dec_contract_1];
    let sorted_storage_diffs = vec![storage_diff_0, storage_diff_1];
    let sorted_declared_classes = vec![dec_class_0, dec_class_1];
    let sorted_nonces = vec![dec_nonce_0, dec_nonce_1];

    assert_eq!(
        state_diff.destruct(),
        (sorted_deployed_contracts, sorted_storage_diffs, sorted_declared_classes, sorted_nonces)
    );
}
