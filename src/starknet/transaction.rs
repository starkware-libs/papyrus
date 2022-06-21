use serde::{Deserialize, Serialize};
use web3::types::H160;

use super::{ContractAddress, StarkFelt, StarkHash};

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionHash(pub StarkHash);

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct Fee(pub u128);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventData(pub Vec<StarkFelt>);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventKey(pub StarkFelt);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Event {
    pub from_address: ContractAddress,
    pub keys: Vec<EventKey>,
    pub data: EventData,
}

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EntryPointSelector(pub StarkHash);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct CallData(pub Vec<StarkFelt>);

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EthAddress(pub H160);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1ToL2Payload(pub Vec<StarkFelt>);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L2ToL1Payload(pub Vec<StarkFelt>);
