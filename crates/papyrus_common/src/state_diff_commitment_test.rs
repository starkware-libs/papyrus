use pretty_assertions::assert_eq;
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    GlobalRoot,
    Nonce,
    PatriciaKey,
};
use starknet_api::state::{StateDiff, StorageKey, ThinStateDiff};
use starknet_api::{class_hash, contract_address, patricia_key};
use starknet_types_core::felt::Felt;

use crate::state_diff_commitment::{calculate_state_diff_commitment, StateDiffVersion};

#[test]
fn state_diff_commitment() {
    let contract_address = contract_address!(0x1);
    let storage_key = StorageKey(patricia_key!(0x1));
    let storage_value = Felt::from_hex_unchecked("0x999");
    let nonce = Nonce(Felt::ONE);
    let class_hash = class_hash!(0x70);
    let compiled_class_hash = CompiledClassHash(Felt::from_hex_unchecked("0x700"));
    let old_class_hash = class_hash!(0x71);
    let replaced_contract_address = contract_address!(0x2);
    let replacing_class_hash = class_hash!(0x72);

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
    let expected_commitment = GlobalRoot(
        Felt::from_hex("0x30eec29bb733bc07197b0e0a41a53808860b2bf9dbb6b4472677a9fc6168a4f")
            .unwrap(),
    );

    assert_eq!(
        calculated_commitment.0.to_fixed_hex_string(),
        expected_commitment.0.to_fixed_hex_string()
    );
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
