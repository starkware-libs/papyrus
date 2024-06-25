use pretty_assertions::assert_eq;
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
    PatriciaKey,
    StateDiffCommitment,
};
use starknet_api::hash::PoseidonHash;
use starknet_api::state::{StateDiff, StorageKey, ThinStateDiff};
use starknet_api::{class_hash, contract_address, felt, patricia_key};

use crate::state_diff_commitment::{calculate_state_diff_commitment, StateDiffVersion};

#[test]
fn state_diff_commitment() {
    let contract_address = contract_address!("0x1");
    let storage_key = StorageKey(patricia_key!("0x1"));
    let storage_value = felt!("0x999");
    let nonce = Nonce(felt!("0x1"));
    let class_hash = class_hash!("0x70");
    let compiled_class_hash = CompiledClassHash(felt!("0x700"));
    let old_class_hash = class_hash!("0x71");
    let replaced_contract_address = contract_address!("0x2");
    let replacing_class_hash = class_hash!("0x72");

    let thin_state_diff = ThinStateDiff {
        deployed_contracts: [(contract_address, class_hash)].into(),
        storage_diffs: [(contract_address, [(storage_key, storage_value)].into())].into(),
        declared_classes: [(class_hash, compiled_class_hash)].into(),
        deprecated_declared_classes: vec![old_class_hash],
        nonces: [(contract_address, nonce)].into(),
        replaced_classes: [(replaced_contract_address, replacing_class_hash)].into(),
    };

    let calculated_commitment =
        calculate_state_diff_commitment(&thin_state_diff, StateDiffVersion::V0);

    // The expected commitment was calculated using the Python implementation of Starknet.
    let expected_commitment = StateDiffCommitment(PoseidonHash(felt!(
        "0x30eec29bb733bc07197b0e0a41a53808860b2bf9dbb6b4472677a9fc6168a4f"
    )));

    assert_eq!(calculated_commitment, expected_commitment);
}

#[test]
fn empty_storage_diff() {
    // TODO: derive default in ThinStateDiff.
    let state_diff = ThinStateDiff::from(StateDiff::default());
    let state_diff_with_empty_storage_diff = ThinStateDiff::from(StateDiff {
        storage_diffs: [(ContractAddress::default(), [].into())].into(),
        ..Default::default()
    });

    assert_ne!(
        calculate_state_diff_commitment(&state_diff, StateDiffVersion::V0),
        calculate_state_diff_commitment(&state_diff_with_empty_storage_diff, StateDiffVersion::V0)
    );
}

#[test]
fn deployed_and_replaced_contracts_are_sorted_for_hashing() {
    // Tests a bug fix where the squashing of deployed and replaced contracts was not sorted by
    // contract address.
    let state_diff = ThinStateDiff {
        deployed_contracts: [(contract_address!("0x2"), class_hash!("0x2"))].into(),
        storage_diffs: [].into(),
        declared_classes: [].into(),
        deprecated_declared_classes: [].into(),
        nonces: [].into(),
        replaced_classes: [
            (contract_address!("0x1"), class_hash!("0x1")),
            (contract_address!("0x3"), class_hash!("0x3")),
        ]
        .into(),
    };

    let calculated_commitment = calculate_state_diff_commitment(&state_diff, StateDiffVersion::V0);
    assert_eq!(
        calculated_commitment.0.0.to_hex_string(),
        "0x5264ab018246d1ab06704c6016285e90962e843561ebb82c4325d7254b1724b"
    );
}
