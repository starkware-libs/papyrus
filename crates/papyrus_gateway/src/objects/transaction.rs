use serde::{Deserialize, Serialize};
use starknet_api::{Transaction, TransactionHash};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum Transactions {
    Hashes(Vec<TransactionHash>),
    Full(Vec<TransactionWithType>),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionType {
    #[serde(rename(deserialize = "DECLARE", serialize = "DECLARE"))]
    Declare,
    #[serde(rename(deserialize = "DEPLOY", serialize = "DEPLOY"))]
    Deploy,
    #[serde(rename(deserialize = "INVOKE", serialize = "INVOKE"))]
    Invoke,
}
impl Default for TransactionType {
    fn default() -> Self {
        TransactionType::Invoke
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionWithType {
    pub r#type: TransactionType,
    #[serde(flatten)]
    pub transaction: Transaction,
}

impl From<Transaction> for TransactionWithType {
    fn from(transaction: Transaction) -> Self {
        match transaction {
            Transaction::Declare(_) => {
                TransactionWithType { r#type: TransactionType::Declare, transaction }
            }
            Transaction::Deploy(_) => {
                TransactionWithType { r#type: TransactionType::Deploy, transaction }
            }
            Transaction::Invoke(_) => {
                TransactionWithType { r#type: TransactionType::Invoke, transaction }
            }
        }
    }
}
