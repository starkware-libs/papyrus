use serde::{Deserialize, Serialize};
use starknet_api::{Transaction, TransactionHash};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum Transactions {
    Hashes(Vec<TransactionHash>),
    Full(Vec<Transaction>),
}
