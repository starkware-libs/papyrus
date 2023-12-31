use pretty_assertions::assert_eq;
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    GlobalRoot,
    Nonce,
    PatriciaKey,
};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_api::{class_hash, contract_address, patricia_key, stark_felt};

use crate::state_diff_commitment::{calculate_state_diff_commitment, StateDiffVersion};

#[test]
fn state_diff_commitment() {
    let contract_address = contract_address!("0x1");
    let storage_key = StorageKey(patricia_key!("0x1"));
    let storage_value = stark_felt!("0x999");
    let nonce = Nonce(stark_felt!("0x1"));
    let class_hash = class_hash!("0x70");
    let compiled_class_hash = CompiledClassHash(stark_felt!("0x700"));
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

    let expected_commitment = GlobalRoot(stark_felt!(
        "0x30eec29bb733bc07197b0e0a41a53808860b2bf9dbb6b4472677a9fc6168a4f"
    ));

    assert_eq!(calculated_commitment, expected_commitment);
}
