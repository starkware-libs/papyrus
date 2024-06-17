use indexmap::indexmap;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_types_core::felt::Felt;

/// A storage entry in a contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    pub key: StorageKey,
    pub value: Felt,
}

/// A deployed contract in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployedContract {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
}

/// A mapping from class hash to the compiled class hash.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeclaredClassHashEntry {
    pub class_hash: ClassHash,
    pub compiled_class_hash: CompiledClassHash,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ReplacedClass {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
}

// TODO: move to used crate
pub fn create_random_state_diff(rng: &mut impl RngCore) -> ThinStateDiff {
    let contract0 = ContractAddress::from(rng.next_u64());
    let contract1 = ContractAddress::from(rng.next_u64());
    let contract2 = ContractAddress::from(rng.next_u64());
    let class_hash = ClassHash(rng.next_u64().into());
    let compiled_class_hash = CompiledClassHash(rng.next_u64().into());
    let deprecated_class_hash = ClassHash(rng.next_u64().into());
    ThinStateDiff {
        deployed_contracts: indexmap! {
            contract0 => class_hash, contract1 => class_hash, contract2 => deprecated_class_hash
        },
        storage_diffs: indexmap! {
            contract0 => indexmap! {
                1u64.into() => Felt::ONE, 2u64.into() => Felt::TWO
            },
            contract1 => indexmap! {
                3u64.into() => Felt::TWO, 4u64.into() => Felt::ONE
            },
        },
        declared_classes: indexmap! { class_hash => compiled_class_hash },
        deprecated_declared_classes: vec![deprecated_class_hash],
        nonces: indexmap! {
            contract0 => Nonce(Felt::ONE), contract2 => Nonce(Felt::TWO)
        },
        replaced_classes: Default::default(),
    }
}
