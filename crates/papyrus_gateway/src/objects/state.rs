use papyrus_storage::ThinStateDiff;
use serde::{Deserialize, Serialize};
use starknet_api::{BlockHash, GlobalRoot};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: ThinStateDiff,
}
