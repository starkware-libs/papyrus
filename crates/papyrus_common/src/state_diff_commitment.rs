#[cfg(test)]
#[path = "state_diff_commitment_test.rs"]
mod state_diff_commitment_test;

use starknet_api::core::GlobalRoot;
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::state::ThinStateDiff;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Poseidon, StarkHash};

const SUPPORTED_DA_MODES_STATE_DIFF_V0: [DataAvailabilityMode; 1] = [DataAvailabilityMode::L1];

/// The version of the state diff for the state diff commitment.
// The version is used to support different data availability modes, currently only L1.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, PartialOrd, Ord)]
pub enum StateDiffVersion {
    #[default]
    V0,
}

impl StateDiffVersion {
    /// Returns the supported data availability modes for the given state diff version.
    pub fn supported_da_modes(&self) -> Vec<DataAvailabilityMode> {
        match self {
            StateDiffVersion::V0 => SUPPORTED_DA_MODES_STATE_DIFF_V0.to_vec(),
        }
    }
}

impl From<StateDiffVersion> for Felt {
    fn from(version: StateDiffVersion) -> Self {
        match version {
            StateDiffVersion::V0 => Felt::ZERO,
        }
    }
}

/// Calculates the state diff commitment.
/// The computation is described here: <https://community.starknet.io/t/introducing-p2p-authentication-and-mismatch-resolution-in-v0-12-2/97993>.
pub fn calculate_state_diff_commitment(
    state_diff: &ThinStateDiff,
    state_diff_version: StateDiffVersion,
) -> GlobalRoot {
    let mut flattened_total_state_diff = vec![Felt::from(state_diff_version)];

    // Deployed contracts and replaced classes are squashed.
    // Assumption: same contract address cannot be found in deployed and replaced in the same state
    // diff.
    let num_deployed_contracts =
        state_diff.deployed_contracts.len() + state_diff.replaced_classes.len();
    let deployed_contracts_iter =
        state_diff.deployed_contracts.iter().chain(state_diff.replaced_classes.iter());
    let mut flattened_deployed_contracts = vec![Felt::from(num_deployed_contracts as u64)];
    for (contract_address, class_hash) in deployed_contracts_iter {
        flattened_deployed_contracts.push(contract_address.0.to_felt());
        flattened_deployed_contracts.push(class_hash.0);
    }
    let hash_of_deployed_contracts = Poseidon::hash_array(&flattened_deployed_contracts);
    flattened_total_state_diff.push(hash_of_deployed_contracts);

    let mut flattened_declared_classes = vec![Felt::from(state_diff.declared_classes.len() as u64)];
    for (class_hash, compiled_class_hash) in &state_diff.declared_classes {
        flattened_declared_classes.push(class_hash.0);
        flattened_declared_classes.push(compiled_class_hash.0);
    }
    let hash_of_declared_classes = Poseidon::hash_array(&flattened_declared_classes);
    flattened_declared_classes.iter().for_each(|val| println!("{}", val.to_fixed_hex_string()));
    println!("\n\n\n");
    flattened_total_state_diff.push(hash_of_declared_classes);

    let mut flattened_deprecated_declared_classes =
        vec![Felt::from(state_diff.deprecated_declared_classes.len() as u64)];
    for class_hash in &state_diff.deprecated_declared_classes {
        flattened_deprecated_declared_classes.push(class_hash.0);
    }
    let hash_of_deprecated_declared_classes =
        Poseidon::hash_array(&flattened_deprecated_declared_classes);
    flattened_total_state_diff.push(hash_of_deprecated_declared_classes);

    let da_modes = state_diff_version.supported_da_modes();
    flattened_total_state_diff.push(Felt::from(da_modes.len() as u64));

    for da_mode in da_modes {
        flattened_total_state_diff.push(Felt::from(da_mode));
        // When more data availability modes are added, the following code should be updated so that
        // only the data of the current mode is included.
        let mut flattened_storage_diffs = vec![Felt::from(state_diff.storage_diffs.len() as u64)];
        for (contract_address, diffs) in &state_diff.storage_diffs {
            flattened_storage_diffs.push(contract_address.0.to_felt());
            flattened_storage_diffs.push(Felt::from(diffs.len() as u64));
            for (key, value) in diffs {
                flattened_storage_diffs.push(key.0.to_felt());
                flattened_storage_diffs.push(*value);
            }
        }
        let mut flattened_nonces = vec![Felt::from(state_diff.nonces.len() as u64)];
        for (contract_address, nonce) in &state_diff.nonces {
            flattened_nonces.push(contract_address.0.to_felt());
            flattened_nonces.push(nonce.0);
        }
        let storage_diffs_and_nonces: Vec<_> =
            flattened_storage_diffs.into_iter().chain(flattened_nonces).collect();
        let hash_of_storage_domain_state_diff =
            Poseidon::hash_array(storage_diffs_and_nonces.as_slice());
        flattened_total_state_diff.push(hash_of_storage_domain_state_diff);
    }
    flattened_total_state_diff.iter().for_each(|val| println!("{}", val.to_fixed_hex_string()));
    GlobalRoot(Poseidon::hash_array(flattened_total_state_diff.as_slice()))
}
