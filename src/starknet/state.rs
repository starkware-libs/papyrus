use serde::{Deserialize, Serialize};

use crate::starknet::block::ClassHash;
use crate::starknet::hash::StarkFelt;
use crate::starknet::ContractAddress;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateDiffForward {
    pub deployed_contracts: Vec<DeployedContract>,
    pub storage_diffs: Vec<(ContractAddress, Vec<(StarkFelt, StarkFelt)>)>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateDiffBackward {
    pub deployed_contracts: Vec<ContractAddress>,
    pub storage_diffs: Vec<(ContractAddress, Vec<(StarkFelt, StarkFelt)>)>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployedContract {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
}
