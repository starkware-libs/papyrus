use assert_matches::assert_matches;

use crate::{
    shash, ClassHash, ContractAddress, ContractClass, ContractNonce, DeclaredContract,
    DeployedContract, Nonce, StarkHash, StarknetApiError, StateDiff, StorageDiff,
};

#[test]
fn state_sorted() {
    let hash0 = shash!("0x0");
    let hash1 = shash!("0x1");

    let dep_contract_0 = DeployedContract {
        address: ContractAddress::try_from(hash0).unwrap(),
        class_hash: ClassHash::new(hash0),
    };
    let dep_contract_1 = DeployedContract {
        address: ContractAddress::try_from(hash1).unwrap(),
        class_hash: ClassHash::new(hash1),
    };
    let storage_diff_0 =
        StorageDiff { address: ContractAddress::try_from(hash0).unwrap(), storage_entries: vec![] };
    let storage_diff_1 =
        StorageDiff { address: ContractAddress::try_from(hash1).unwrap(), storage_entries: vec![] };

    let dec_contract_0 = DeclaredContract {
        class_hash: ClassHash::new(hash0),
        contract_class: ContractClass::default(),
    };
    let dec_contract_1 = DeclaredContract {
        class_hash: ClassHash::new(hash1),
        contract_class: ContractClass::default(),
    };

    let nonce_0 = ContractNonce {
        contract_address: ContractAddress::try_from(hash0).unwrap(),
        nonce: Nonce::new(hash0),
    };
    let nonce_1 = ContractNonce {
        contract_address: ContractAddress::try_from(hash1).unwrap(),
        nonce: Nonce::new(hash1),
    };

    let unsorted_deployed_contracts = vec![dep_contract_1.clone(), dep_contract_0.clone()];
    let unsorted_storage_diffs = vec![storage_diff_1.clone(), storage_diff_0.clone()];
    let unsorted_declared_contracts = vec![dec_contract_1.clone(), dec_contract_0.clone()];
    let unsorted_nonces = vec![nonce_1.clone(), nonce_0.clone()];

    let state_diff = StateDiff::new(
        unsorted_deployed_contracts,
        unsorted_storage_diffs,
        unsorted_declared_contracts,
        unsorted_nonces,
    )
    .unwrap();

    let sorted_deployed_contracts = vec![dep_contract_0, dep_contract_1];
    let sorted_storage_diffs = vec![storage_diff_0, storage_diff_1];
    let sorted_declared_contracts = vec![dec_contract_0, dec_contract_1];
    let sorted_nonces = vec![nonce_0, nonce_1];

    assert_eq!(
        state_diff.destruct(),
        (sorted_deployed_contracts, sorted_storage_diffs, sorted_declared_contracts, sorted_nonces)
    );
}

#[test]
fn state_unique() -> Result<(), anyhow::Error> {
    let hash0 = shash!("0x0");

    let dep_contract = DeployedContract {
        address: ContractAddress::try_from(hash0).unwrap(),
        class_hash: ClassHash::new(hash0),
    };

    let storage_diff =
        StorageDiff { address: ContractAddress::try_from(hash0).unwrap(), storage_entries: vec![] };

    let dec_contract = DeclaredContract {
        class_hash: ClassHash::new(hash0),
        contract_class: ContractClass::default(),
    };

    let nonce = ContractNonce {
        contract_address: ContractAddress::try_from(hash0).unwrap(),
        nonce: Nonce::new(hash0),
    };

    let deployed_contracts = vec![dep_contract];
    let storage_diffs = vec![storage_diff];
    let declared_contracts = vec![dec_contract];
    let nonces = vec![nonce];

    let state_diff = StateDiff::new(
        deployed_contracts,
        storage_diffs,
        declared_contracts,
        nonces,
    )?;

    let hash1 = shash!("0x1");
    let duplicates = (
        DeployedContract {
            address: ContractAddress::try_from(hash0).unwrap(),
            class_hash: ClassHash::new(hash1),
        },
        StorageDiff { address: ContractAddress::try_from(hash0).unwrap(), storage_entries: vec![] },
        DeclaredContract {
            class_hash: ClassHash::new(hash0),
            contract_class: ContractClass::default(),
        },
        ContractNonce {
            contract_address: ContractAddress::try_from(hash0).unwrap(),
            nonce: Nonce::new(hash1),
        },
    );

    let mut state_diff_with_duplicate_destructed = state_diff.clone().destruct();
    state_diff_with_duplicate_destructed.0.push(duplicates.0);
    let state_diff_with_duplicate = StateDiff::new(
        state_diff_with_duplicate_destructed.0,
        state_diff_with_duplicate_destructed.1,
        state_diff_with_duplicate_destructed.2,
        state_diff_with_duplicate_destructed.3,
    );
    assert_matches!(state_diff_with_duplicate, Err(StarknetApiError::DuplicateInStateDiff{object}) if object == "deployed_contracts");

    let mut state_diff_with_duplicate_destructed = state_diff.clone().destruct();
    state_diff_with_duplicate_destructed.1.push(duplicates.1);
    let state_diff_with_duplicate = StateDiff::new(
        state_diff_with_duplicate_destructed.0,
        state_diff_with_duplicate_destructed.1,
        state_diff_with_duplicate_destructed.2,
        state_diff_with_duplicate_destructed.3,
    );
    assert_matches!(state_diff_with_duplicate, Err(StarknetApiError::DuplicateInStateDiff{object}) if object == "storage_diffs");

    let mut state_diff_with_duplicate_destructed = state_diff.clone().destruct();
    state_diff_with_duplicate_destructed.2.push(duplicates.2);
    let state_diff_with_duplicate = StateDiff::new(
        state_diff_with_duplicate_destructed.0,
        state_diff_with_duplicate_destructed.1,
        state_diff_with_duplicate_destructed.2,
        state_diff_with_duplicate_destructed.3,
    );
    assert_matches!(state_diff_with_duplicate, Err(StarknetApiError::DuplicateInStateDiff{object}) if object == "declared_contracts");

    let mut state_diff_with_duplicate_destructed = state_diff.destruct();
    state_diff_with_duplicate_destructed.3.push(duplicates.3);
    let state_diff_with_duplicate = StateDiff::new(
        state_diff_with_duplicate_destructed.0,
        state_diff_with_duplicate_destructed.1,
        state_diff_with_duplicate_destructed.2,
        state_diff_with_duplicate_destructed.3,
    );
    assert_matches!(state_diff_with_duplicate, Err(StarknetApiError::DuplicateInStateDiff{object}) if object == "nonces");

    Ok(())
}
