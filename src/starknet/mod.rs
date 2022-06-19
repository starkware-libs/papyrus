mod block;
mod hash;
mod state;
mod transaction;

use serde::{Deserialize, Serialize};

pub use block::{
    BlockBody, BlockHash, BlockHeader, BlockNumber, BlockTimestamp, EventsCommitment, GasPrice,
    GlobalRoot, TransactionsCommitment,
};
pub use hash::{StarkFelt, StarkHash};
pub use state::{DeployedContract, StateDiffBackward, StateDiffForward};
pub use transaction::{
    CallData, EntryPointSelector, EthAddress, Event, Fee, L1ToL2Payload, L2ToL1Payload,
    TransactionHash,
};

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddress(pub StarkHash);
