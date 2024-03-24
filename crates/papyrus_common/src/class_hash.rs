#[cfg(test)]
#[path = "class_hash_test.rs"]
mod class_hash_test;
use lazy_static::lazy_static;
use sha3::Digest;
use starknet_api::core::ClassHash;
use starknet_api::hash::{poseidon_hash_array, PoseidonHash, StarkFelt};
use starknet_api::state::{ContractClass, EntryPointType};
use starknet_crypto::FieldElement;

use crate::usize_into_felt;

lazy_static! {
    static ref API_VERSION: StarkFelt = StarkFelt::from(
        FieldElement::from_byte_slice_be(b"CONTRACT_CLASS_V0.1.0")
            .expect("CONTRACT_CLASS_V0.1.0 is valid StarkFelt."),
    );
}

/// Calculates the hash of a contract class.
// Based on Pathfinder code (the starknet.io doc is incorrect).
pub fn calculate_class_hash(class: &ContractClass) -> ClassHash {
    let external_entry_points_hash = entry_points_hash(class, &EntryPointType::External);
    let l1_handler_entry_points_hash = entry_points_hash(class, &EntryPointType::L1Handler);
    let constructor_entry_points_hash = entry_points_hash(class, &EntryPointType::Constructor);
    let abi_keccak = sha3::Keccak256::default().chain_update(class.abi.as_bytes()).finalize();
    let abi_hash = truncated_keccak(abi_keccak.into());
    let program_hash = poseidon_hash_array(class.sierra_program.as_slice());

    let class_hash = poseidon_hash_array(&[
        *API_VERSION,
        external_entry_points_hash.0,
        l1_handler_entry_points_hash.0,
        constructor_entry_points_hash.0,
        abi_hash,
        program_hash.0,
    ]);
    // TODO: Modify ClassHash Be be PoseidonHash instead of StarkFelt.
    ClassHash(class_hash.0)
}

fn entry_points_hash(class: &ContractClass, entry_point_type: &EntryPointType) -> PoseidonHash {
    poseidon_hash_array(
        class
            .entry_points_by_type
            .get(entry_point_type)
            .unwrap_or(&vec![])
            .iter()
            .flat_map(|ep| [ep.selector.0, usize_into_felt(ep.function_idx.0)])
            .collect::<Vec<_>>()
            .as_slice(),
    )
}

// Python code masks with (2**250 - 1) which starts 0x03 and is followed by 31 0xff in be.
// Truncation is needed not to overflow the field element.
fn truncated_keccak(mut plain: [u8; 32]) -> StarkFelt {
    plain[0] &= 0x03;
    StarkFelt::new_unchecked(plain)
}
